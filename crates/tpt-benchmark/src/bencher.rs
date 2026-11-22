//! The benchmark execution loop: warm-up, batch calibration and adaptive sampling.

use std::hint::black_box;
use std::time::{Duration, Instant};

use crate::engine::{Observations, SequentialEngine, StatisticalEngine};
use crate::layer::{ErasedLayer, MeasurementLayer};
use crate::stats::{Summary, Welford, ci_half_width_95, median};

/// Tunables for one measurement run.
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    /// Time spent running the routine before any sample is recorded.
    pub warmup: Duration,
    /// A batch must take at least this long; calibration doubles the
    /// iteration count until it does, which bounds timer overhead.
    pub min_batch: Duration,
    /// Upper bound on iterations per batch, to stop runaway calibration.
    pub max_batch_iters: u64,
    /// The sequential statistical engine that decides sampling and stopping.
    pub engine: SequentialEngine,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            warmup: Duration::from_millis(100),
            min_batch: Duration::from_millis(1),
            max_batch_iters: 1 << 24,
            engine: SequentialEngine::default(),
        }
    }
}

/// Execution context handed to a benchmark function.
///
/// Call [`Bencher::iter`] exactly once with the routine to measure.
pub struct Bencher<'a> {
    layer: &'a mut dyn ErasedLayer,
    config: &'a Config,
    result: Option<Summary>,
}

impl std::fmt::Debug for Bencher<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bencher")
            .field("layer", &self.layer.name())
            .field("config", &self.config)
            .field("result", &self.result)
            .finish()
    }
}

impl Bencher<'_> {
    /// Repeatedly runs `routine` and records its per-iteration cost.
    ///
    /// The routine's return value is passed through [`black_box`] so the
    /// optimiser cannot delete the work being measured.
    ///
    /// # Panics
    ///
    /// Panics if `config.engine.min_samples` is 0 or `max_samples` is below
    /// `min_samples`. Those settings cannot produce a meaningful result.
    pub fn iter<O, F: FnMut() -> O>(&mut self, mut routine: F) {
        let engine = &self.config.engine;
        assert!(engine.min_samples > 0, "min_samples must be at least 1");
        assert!(
            engine.max_samples >= engine.min_samples,
            "max_samples must be at least min_samples"
        );

        let layer = &mut *self.layer;
        let mut run_batch = |iters: u64| -> f64 {
            layer.start_batch();
            for _ in 0..iters {
                black_box(routine());
            }
            layer.end_batch()
        };

        // Warm-up: caches, branch predictors and frequency scaling settle.
        let warm_start = Instant::now();
        while warm_start.elapsed() < self.config.warmup {
            run_batch(1);
        }

        let iters = calibrate(&mut run_batch, self.config);

        let mut accepted: Vec<f64> = Vec::new();
        let mut flags: Vec<bool> = Vec::new();
        let sampling_start = Instant::now();
        let stop_reason = loop {
            let per_iter = run_batch(iters) / iters as f64;
            let outlier = engine.is_outlier(&accepted, per_iter);
            flags.push(outlier);
            if !outlier {
                accepted.push(per_iter);
            }
            let obs = Observations {
                accepted: &accepted,
                outlier_flags: &flags,
                elapsed: sampling_start.elapsed(),
            };
            if let Some(reason) = engine.should_stop(&obs) {
                break reason;
            }
        };

        let w = Welford::from_slice(&accepted);
        let half = ci_half_width_95(&w);
        let noisy = engine.noise_verdict(&flags) != crate::engine::SprtDecision::Clean;
        self.result = Some(Summary {
            layer: self.layer.name().to_owned(),
            mean: w.mean(),
            std_dev: w.std_dev(),
            median: median(&accepted),
            ci_low: w.mean() - half,
            ci_high: w.mean() + half,
            samples: accepted.len() as u64,
            values: accepted,
            outliers: flags.iter().filter(|&&f| f).count() as u64,
            iters_per_batch: iters,
            stop_reason,
            noisy,
        });
    }
}

/// Doubles the batch size until one batch takes at least `config.min_batch`,
/// starting from 10 iterations (or the cap, whichever is lower).
fn calibrate(run_batch: &mut impl FnMut(u64) -> f64, config: &Config) -> u64 {
    let min_ns = config.min_batch.as_nanos() as f64;
    let mut iters: u64 = 10.min(config.max_batch_iters);
    loop {
        let total = run_batch(iters);
        if total >= min_ns || iters >= config.max_batch_iters {
            return iters;
        }
        iters = (iters * 2).min(config.max_batch_iters);
    }
}

/// Measures one layer of a benchmark and returns its summary.
///
/// `f` is the benchmark function. It must call [`Bencher::iter`] exactly once.
/// To activate several layers, call `measure` once per layer. Each call runs
/// the function again, which is how the layers stay independent.
///
/// # Panics
///
/// Panics if `f` does not call [`Bencher::iter`], or if `config` is invalid
/// (see [`Bencher::iter`]).
pub fn measure<L, F>(config: &Config, layer: L, mut f: F) -> Summary
where
    L: MeasurementLayer,
    F: FnMut(&mut Bencher<'_>),
{
    let mut layer = layer;
    let mut bencher = Bencher {
        layer: &mut layer,
        config,
        result: None,
    };
    f(&mut bencher);
    bencher
        .result
        .expect("benchmark function did not call Bencher::iter")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::Clock;
    use crate::stats::StopReason;
    use crate::wallclock::WallClock;

    fn fast_config() -> Config {
        Config {
            warmup: Duration::from_millis(1),
            min_batch: Duration::from_micros(200),
            max_batch_iters: 1 << 20,
            engine: SequentialEngine {
                max_time: Duration::from_secs(2),
                ..SequentialEngine::default()
            },
        }
    }

    #[test]
    fn measures_a_known_sleep_close_to_its_duration() {
        let cfg = fast_config();
        let summary = measure(&cfg, WallClock::with_clock(Clock::best()), |b| {
            b.iter(|| std::thread::sleep(Duration::from_micros(500)));
        });
        assert_eq!(summary.layer, "WallClock");
        assert!(summary.samples >= cfg.engine.min_samples);
        // Sleep overshoots, so the mean must be at least the requested time.
        assert!(summary.mean >= 500_000.0, "mean = {}", summary.mean);
        assert!(summary.ci_low <= summary.mean && summary.mean <= summary.ci_high);
    }

    #[test]
    fn a_zero_work_routine_produces_a_summary() {
        let summary = measure(
            &fast_config(),
            WallClock::with_clock(Clock::instant()),
            |b| {
                b.iter(|| 1_u64 + 1);
            },
        );
        assert!(summary.mean >= 0.0);
        assert!(summary.iters_per_batch >= 10);
        assert!(matches!(
            summary.stop_reason,
            StopReason::Converged | StopReason::MaxSamples | StopReason::MaxTime
        ));
    }

    #[test]
    #[should_panic(expected = "did not call Bencher::iter")]
    fn missing_iter_call_panics() {
        let _ = measure(
            &fast_config(),
            WallClock::with_clock(Clock::instant()),
            |_b| {},
        );
    }

    #[test]
    #[should_panic(expected = "max_samples must be at least min_samples")]
    fn invalid_engine_config_panics() {
        let mut cfg = fast_config();
        cfg.engine.max_samples = 1;
        let _ = measure(&cfg, WallClock::with_clock(Clock::instant()), |b| {
            b.iter(|| 0_u8);
        });
    }
}
