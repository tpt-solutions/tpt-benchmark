//! The benchmark execution loop: warm-up, batch calibration and adaptive sampling.

use std::hint::black_box;
use std::time::{Duration, Instant};

use crate::engine::{Observations, SequentialEngine, StatisticalEngine};
use crate::layer::{ErasedLayer, MeasurementLayer};
use crate::stats::{Summary, Welford, ci_half_width_95, median};
use crate::throughput::Throughput;
use std::future::Future;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

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
    throughput: Option<Throughput>,
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
    /// Sets the work done per iteration, so the summary also reports a rate.
    pub fn throughput(&mut self, throughput: Throughput) {
        self.throughput = Some(throughput);
    }

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
        let max_iters = self.config.max_batch_iters;
        let config = self.config;
        let name = self.layer.name();
        let throughput = self.throughput;
        let layer = &mut *self.layer;
        let run_batch = move |iters: u64| -> f64 {
            layer.start_batch();
            for _ in 0..iters {
                black_box(routine());
            }
            layer.end_batch()
        };
        self.result = Some(sample(config, name, throughput, run_batch, max_iters));
    }

    /// Like [`iter`](Self::iter), but `setup` builds the input for each
    /// iteration outside the timed region.
    ///
    /// Inputs for one batch are built together before the batch starts, so
    /// the batch is capped at 1024 iterations to bound memory. Dropping the
    /// outputs happens inside the timed region.
    pub fn iter_batched<I, O, S, R>(&mut self, mut setup: S, mut routine: R)
    where
        S: FnMut() -> I,
        R: FnMut(I) -> O,
    {
        const MAX_BATCH_INPUTS: u64 = 1024;
        let max_iters = self.config.max_batch_iters.min(MAX_BATCH_INPUTS);
        let config = self.config;
        let name = self.layer.name();
        let throughput = self.throughput;
        let layer = &mut *self.layer;
        let run_batch = move |iters: u64| -> f64 {
            let mut inputs: Vec<I> = (0..iters).map(|_| setup()).collect();
            layer.start_batch();
            for input in inputs.drain(..) {
                black_box(routine(input));
            }
            layer.end_batch()
        };
        self.result = Some(sample(config, name, throughput, run_batch, max_iters));
    }

    /// Measures an async routine, polling its futures with a built-in
    /// executor.
    ///
    /// The built-in executor busy-polls with a no-op waker. It suits futures
    /// that complete without an external reactor. For a real runtime, use
    /// [`iter_async_with`](Self::iter_async_with).
    pub fn iter_async<F, Fut>(&mut self, routine: F)
    where
        F: FnMut() -> Fut,
        Fut: Future,
    {
        self.iter_async_with(block_on_busy, routine);
    }

    /// Measures an async routine, driving each future with `executor`.
    ///
    /// The executor's own overhead is part of each measurement. Use the
    /// runtime's `block_on`, for example `|f| runtime.block_on(f)`.
    pub fn iter_async_with<E, F, Fut>(&mut self, mut executor: E, mut routine: F)
    where
        E: FnMut(Fut) -> Fut::Output,
        F: FnMut() -> Fut,
        Fut: Future,
    {
        let max_iters = self.config.max_batch_iters;
        let config = self.config;
        let name = self.layer.name();
        let throughput = self.throughput;
        let layer = &mut *self.layer;
        let run_batch = move |iters: u64| -> f64 {
            layer.start_batch();
            for _ in 0..iters {
                black_box(executor(routine()));
            }
            layer.end_batch()
        };
        self.result = Some(sample(config, name, throughput, run_batch, max_iters));
    }
}

/// Warm-up, calibration, adaptive sampling and summary. Every public
/// `iter*` method calls this with its own batch closure.
fn sample(
    config: &Config,
    layer_name: &str,
    throughput: Option<Throughput>,
    mut run_batch: impl FnMut(u64) -> f64,
    max_iters: u64,
) -> Summary {
    let engine = &config.engine;
    assert!(engine.min_samples > 0, "min_samples must be at least 1");
    assert!(
        engine.max_samples >= engine.min_samples,
        "max_samples must be at least min_samples"
    );

    // Warm-up: caches, branch predictors and frequency scaling settle.
    let warm_start = Instant::now();
    while warm_start.elapsed() < config.warmup {
        run_batch(1);
    }

    let iters = calibrate(&mut run_batch, config, max_iters);

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
    Summary {
        layer: layer_name.to_owned(),
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
        throughput,
    }
}

/// Busy-polls a future with a no-op waker until it completes.
fn block_on_busy<Fut: Future>(fut: Fut) -> Fut::Output {
    fn noop_raw_waker() -> RawWaker {
        fn clone(_: *const ()) -> RawWaker {
            noop_raw_waker()
        }
        fn noop(_: *const ()) {}
        static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, noop, noop, noop);
        RawWaker::new(std::ptr::null(), &VTABLE)
    }
    // SAFETY: every vtable function is a no-op that ignores the data pointer,
    // so the waker meets the RawWaker contract for any use.
    #[allow(unsafe_code)]
    let waker = unsafe { Waker::from_raw(noop_raw_waker()) };
    let mut cx = Context::from_waker(&waker);
    let mut fut = std::pin::pin!(fut);
    loop {
        if let Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
            return out;
        }
    }
}

/// Doubles the batch size until one batch takes at least `config.min_batch`,
/// starting from 10 iterations (or the cap, whichever is lower).
fn calibrate(run_batch: &mut impl FnMut(u64) -> f64, config: &Config, max_iters: u64) -> u64 {
    let min_ns = config.min_batch.as_nanos() as f64;
    let mut iters: u64 = 10.min(max_iters);
    loop {
        let total = run_batch(iters);
        if total >= min_ns || iters >= max_iters {
            return iters;
        }
        iters = (iters * 2).min(max_iters);
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
        throughput: None,
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

    #[test]
    fn iter_batched_excludes_setup_time() {
        // Setup burns about 200 µs per input; the routine is trivial. The
        // reported mean must be far below the setup cost.
        let summary = measure(
            &fast_config(),
            WallClock::with_clock(Clock::instant()),
            |b| {
                b.iter_batched(
                    || std::thread::sleep(Duration::from_micros(200)),
                    |()| 1_u64,
                );
            },
        );
        assert!(
            summary.mean < 50_000.0,
            "mean = {} ns includes setup",
            summary.mean
        );
        assert!(summary.iters_per_batch <= 1024);
    }

    #[test]
    fn iter_async_runs_a_ready_future() {
        let mut calls = 0_u64;
        let summary = measure(
            &fast_config(),
            WallClock::with_clock(Clock::instant()),
            |b| {
                b.iter_async(|| {
                    calls += 1;
                    async { 7_u64 }
                });
            },
        );
        assert!(calls > 0);
        assert!(summary.samples >= 10);
    }

    #[test]
    fn iter_async_with_uses_the_supplied_executor() {
        let mut executor_calls = 0_u64;
        let _ = measure(
            &fast_config(),
            WallClock::with_clock(Clock::instant()),
            |b| {
                b.iter_async_with(
                    |fut| {
                        executor_calls += 1;
                        block_on_busy(fut)
                    },
                    || async { 1_u8 },
                );
            },
        );
        assert!(executor_calls > 0);
    }

    #[test]
    fn throughput_is_reported_in_the_summary() {
        let summary = measure(
            &fast_config(),
            WallClock::with_clock(Clock::instant()),
            |b| {
                b.throughput(Throughput::Bytes(64));
                b.iter(|| 1_u8);
            },
        );
        assert_eq!(summary.throughput, Some(Throughput::Bytes(64)));
    }
}
