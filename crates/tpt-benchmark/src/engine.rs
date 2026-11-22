//! The sequential statistical engine: decides which samples to keep and when to stop.
//!
//! Three mechanisms cooperate:
//!
//! - **Outlier rejection (MAD).** A candidate sample further than `k` scaled
//!   MADs from the median of the accepted samples is rejected in real time.
//!   The rejection is recorded as a noise indicator for the SPRT below.
//! - **Precision stopping.** Sampling stops once the 95 % confidence interval
//!   of the mean is within `rel_error` of the mean, but only after a
//!   minimum number of samples and only when the noise test is clean.
//! - **Noise-regime SPRT.** Wald's sequential probability ratio test on the
//!   outlier indicator stream. H0: outlier rate `p0` (clean environment).
//!   H1: outlier rate `p1` (noisy environment). Accepting H0 is what lets
//!   precision stopping fire; accepting H1 marks the result as noisy.

use std::time::Duration;

use crate::stats::{StopReason, Welford, ci_half_width_95, median, scaled_mad};

/// Everything the engine may consult when deciding whether to stop.
#[derive(Clone, Copy, Debug)]
pub struct Observations<'a> {
    /// Per-iteration values of accepted samples, in arrival order.
    pub accepted: &'a [f64],
    /// Outlier flag for every draw, accepted or not, in arrival order.
    pub outlier_flags: &'a [bool],
    /// Wall time spent sampling so far.
    pub elapsed: Duration,
}

/// Outcome of the noise-regime SPRT.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SprtDecision {
    /// H0 accepted: the outlier rate is consistent with a clean environment.
    Clean,
    /// H1 accepted: the outlier rate is consistent with a noisy environment.
    Noisy,
    /// Neither hypothesis is decided yet.
    Undecided,
}

/// Parameters for Wald's SPRT on the outlier indicator stream.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sprt {
    /// Outlier rate under H0 (clean).
    pub p0: f64,
    /// Outlier rate under H1 (noisy).
    pub p1: f64,
    /// Type I error bound.
    pub alpha: f64,
    /// Type II error bound.
    pub beta: f64,
}

impl Default for Sprt {
    fn default() -> Self {
        Self {
            p0: 0.02,
            p1: 0.20,
            alpha: 0.05,
            beta: 0.05,
        }
    }
}

impl Sprt {
    /// Runs the test over a stream of outlier flags.
    #[must_use]
    pub fn decide(&self, outlier_flags: &[bool]) -> SprtDecision {
        let llr_hit = (self.p1 / self.p0).ln();
        let llr_miss = ((1.0 - self.p1) / (1.0 - self.p0)).ln();
        let lower = (self.beta / (1.0 - self.alpha)).ln();
        let upper = ((1.0 - self.beta) / self.alpha).ln();

        let mut llr = 0.0;
        for &flag in outlier_flags {
            llr += if flag { llr_hit } else { llr_miss };
            if llr <= lower {
                return SprtDecision::Clean;
            }
            if llr >= upper {
                return SprtDecision::Noisy;
            }
        }
        SprtDecision::Undecided
    }
}

/// Decides outlier status and when sampling should stop.
pub trait StatisticalEngine {
    /// Returns `true` if `candidate` should be rejected, given the samples
    /// accepted so far.
    fn is_outlier(&self, accepted: &[f64], candidate: f64) -> bool;

    /// Returns the reason to stop, or `None` to keep sampling.
    fn should_stop(&self, observations: &Observations<'_>) -> Option<StopReason>;

    /// Noise verdict for the samples gathered so far.
    fn noise_verdict(&self, outlier_flags: &[bool]) -> SprtDecision;
}

/// The default engine: MAD rejection, precision stopping, SPRT noise gate.
#[derive(Clone, Debug, PartialEq)]
pub struct SequentialEngine {
    /// Target relative half-width of the 95 % CI, e.g. `0.02` for ±2 %.
    pub rel_error: f64,
    /// Minimum accepted samples before precision stopping may fire.
    pub min_samples: u64,
    /// Maximum total draws (accepted plus rejected).
    pub max_samples: u64,
    /// Maximum wall time spent sampling.
    pub max_time: Duration,
    /// Outlier threshold in scaled MADs.
    pub outlier_k: f64,
    /// Parameters for the noise-regime SPRT.
    pub sprt: Sprt,
}

impl Default for SequentialEngine {
    fn default() -> Self {
        Self {
            rel_error: 0.02,
            min_samples: 10,
            max_samples: 1_000,
            max_time: Duration::from_secs(5),
            outlier_k: 5.0,
            sprt: Sprt::default(),
        }
    }
}

/// Fewest accepted samples before outlier rejection starts.
///
/// Below this the MAD is too noisy to reject against. Ordinary draws would be
/// flagged, and a couple of those early false flags are enough to make the
/// SPRT call a clean stream noisy.
const MIN_SAMPLES_FOR_MAD: usize = 20;

impl StatisticalEngine for SequentialEngine {
    fn is_outlier(&self, accepted: &[f64], candidate: f64) -> bool {
        if accepted.len() < MIN_SAMPLES_FOR_MAD {
            return false;
        }
        let med = median(accepted);
        // Floor the scale at 0.5 % of the median. Timer quantisation can
        // make the MAD exactly zero, and an unfloored scale would then reject
        // every sample that differs by one tick.
        let scale = scaled_mad(accepted).max(med.abs() * 0.005);
        if scale <= 0.0 || !scale.is_finite() {
            return false;
        }
        (candidate - med).abs() > self.outlier_k * scale
    }

    fn should_stop(&self, obs: &Observations<'_>) -> Option<StopReason> {
        let draws = obs.outlier_flags.len() as u64;
        let accepted = obs.accepted.len() as u64;

        if accepted >= self.min_samples
            && self.precision_met(obs.accepted)
            && self.noise_verdict(obs.outlier_flags) == SprtDecision::Clean
        {
            return Some(StopReason::Converged);
        }
        if draws >= self.max_samples {
            return Some(StopReason::MaxSamples);
        }
        if obs.elapsed >= self.max_time {
            return Some(StopReason::MaxTime);
        }
        None
    }

    fn noise_verdict(&self, outlier_flags: &[bool]) -> SprtDecision {
        // Draws during burn-in are never tested, so they say nothing about the
        // environment. Scoring them as clean would let the SPRT accept H0
        // before any real evidence exists.
        let scored = outlier_flags.get(MIN_SAMPLES_FOR_MAD..).unwrap_or(&[]);
        self.sprt.decide(scored)
    }
}

impl SequentialEngine {
    fn precision_met(&self, accepted: &[f64]) -> bool {
        let w = Welford::from_slice(accepted);
        let mean = w.mean();
        let half = ci_half_width_95(&w);
        // Zero spread means every accepted value is identical, so the
        // interval has no width. That is precise by definition, even when the
        // mean is zero (e.g. a zero-allocation routine).
        if half == 0.0 {
            return true;
        }
        if !mean.is_finite() || mean == 0.0 {
            return false;
        }
        half / mean.abs() <= self.rel_error
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn obs<'a>(accepted: &'a [f64], flags: &'a [bool], secs: u64) -> Observations<'a> {
        Observations {
            accepted,
            outlier_flags: flags,
            elapsed: Duration::from_secs(secs),
        }
    }

    #[test]
    fn sprt_accepts_clean_stream() {
        let flags = vec![false; 300];
        assert_eq!(Sprt::default().decide(&flags), SprtDecision::Clean);
    }

    #[test]
    fn sprt_flags_noisy_stream() {
        // A 30 % outlier rate is far above p1 = 0.20.
        let flags: Vec<bool> = (0..200).map(|i| i % 3 == 0).collect();
        assert_eq!(Sprt::default().decide(&flags), SprtDecision::Noisy);
    }

    #[test]
    fn sprt_is_undecided_on_short_streams() {
        assert_eq!(
            Sprt::default().decide(&[false, false]),
            SprtDecision::Undecided
        );
    }

    #[test]
    fn outlier_rejects_a_spike_and_keeps_inliers() {
        let engine = SequentialEngine::default();
        let accepted: Vec<f64> = (0..25)
            .map(|i| 10.0 + 0.1 * f64::from(i % 5) - 0.2)
            .collect();
        assert!(engine.is_outlier(&accepted, 50.0));
        assert!(!engine.is_outlier(&accepted, 10.05));
    }

    #[test]
    fn quantised_timer_does_not_reject_one_tick_jitter() {
        // Identical values give a zero MAD; the 0.5 % floor must keep a
        // one-tick difference from being treated as an outlier.
        let engine = SequentialEngine::default();
        let accepted = [1000.0; 8];
        assert!(!engine.is_outlier(&accepted, 1001.0));
    }

    #[test]
    fn no_rejection_before_enough_samples() {
        let engine = SequentialEngine::default();
        assert!(!engine.is_outlier(&[10.0, 10.0], 1_000.0));
    }

    #[test]
    fn converges_on_tight_clean_data() {
        let engine = SequentialEngine::default();
        let accepted: Vec<f64> = (0..50).map(|i| 100.0 + (i % 3) as f64 * 0.01).collect();
        let flags = vec![false; accepted.len()];
        assert_eq!(
            engine.should_stop(&obs(&accepted, &flags, 0)),
            Some(StopReason::Converged)
        );
    }

    #[test]
    fn does_not_stop_below_min_samples() {
        let engine = SequentialEngine::default();
        let accepted = [100.0, 100.0, 100.0];
        let flags = [false, false, false];
        assert_eq!(engine.should_stop(&obs(&accepted, &flags, 0)), None);
    }

    #[test]
    fn caps_stop_noisy_runs() {
        let engine = SequentialEngine {
            max_samples: 20,
            ..SequentialEngine::default()
        };
        // Wide spread never meets precision, and the flags look noisy.
        let accepted: Vec<f64> = (0..20).map(|i| 100.0 + (i as f64) * 30.0).collect();
        let flags: Vec<bool> = (0..20).map(|i| i % 3 == 0).collect();
        assert_eq!(
            engine.should_stop(&obs(&accepted, &flags, 0)),
            Some(StopReason::MaxSamples)
        );
    }

    #[test]
    fn time_cap_stops_sampling() {
        let engine = SequentialEngine::default();
        let accepted = [1.0, 50.0, 3.0];
        let flags = [false, false, false];
        assert_eq!(
            engine.should_stop(&obs(&accepted, &flags, 10)),
            Some(StopReason::MaxTime)
        );
    }
}

#[cfg(test)]
mod zero_spread {
    use super::*;

    #[test]
    fn constant_zero_stream_converges() {
        // A zero-allocation routine reports 0 every batch. Zero spread must
        // count as precise, or the run would only end at the sample cap.
        let engine = SequentialEngine::default();
        // The SPRT needs about 31 clean draws before it accepts H0.
        let accepted = vec![0.0; 40];
        let flags = vec![false; 40];
        let obs = Observations {
            accepted: &accepted,
            outlier_flags: &flags,
            elapsed: Duration::ZERO,
        };
        assert_eq!(engine.should_stop(&obs), Some(StopReason::Converged));
    }
}

#[cfg(test)]
mod validation {
    use super::*;
    use crate::stats::testutil::Rng;
    use crate::stats::{Welford, ci_half_width_95};

    /// Synthetic Gaussian data: the 95 % interval should cover the true mean
    /// in about 95 % of trials. A 5 % tolerance band keeps the test stable.
    #[test]
    fn ci_coverage_is_close_to_nominal() {
        let trials = 2_000;
        let n = 30;
        let true_mean = 100.0;
        let mut rng = Rng::new(0x00C0_FFEE);
        let mut covered = 0;
        for _ in 0..trials {
            let xs: Vec<f64> = (0..n).map(|_| true_mean + 5.0 * rng.normal()).collect();
            let w = Welford::from_slice(&xs);
            let half = ci_half_width_95(&w);
            if (w.mean() - true_mean).abs() <= half {
                covered += 1;
            }
        }
        let rate = f64::from(covered) / f64::from(trials);
        assert!((0.93..=0.97).contains(&rate), "coverage = {rate}");
    }

    /// On clean Gaussian data the outlier rate at 5 scaled MADs is
    /// essentially zero, well under the SPRT's clean rate p0 = 2 %. Almost every stream
    /// should therefore be accepted as clean, not flagged as noisy.
    #[test]
    fn clean_gaussian_streams_are_not_flagged_as_noisy() {
        let engine = SequentialEngine::default();
        let mut rng = Rng::new(0x0BAD_C0DE);
        let streams = 200;
        let mut flagged_noisy = 0;
        for _ in 0..streams {
            let mut accepted: Vec<f64> = Vec::new();
            let mut flags: Vec<bool> = Vec::new();
            for _ in 0..400 {
                let x = 100.0 + 2.0 * rng.normal();
                let outlier = engine.is_outlier(&accepted, x);
                flags.push(outlier);
                if !outlier {
                    accepted.push(x);
                }
            }
            if engine.noise_verdict(&flags) == SprtDecision::Noisy {
                flagged_noisy += 1;
            }
        }
        // Allow the SPRT's own alpha (5 %) plus slack.
        assert!(flagged_noisy <= 15, "flagged {flagged_noisy} of {streams}");
    }

    /// Burn-in draws are never outlier-tested, so twenty of them must not be
    /// enough for the SPRT to call the environment clean.
    #[test]
    fn burn_in_draws_do_not_decide_the_verdict() {
        let engine = SequentialEngine::default();
        let flags = vec![false; MIN_SAMPLES_FOR_MAD];
        assert_eq!(engine.noise_verdict(&flags), SprtDecision::Undecided);
    }

    /// A stream with 25 % outliers is clearly noisy and must be flagged.
    #[test]
    fn heavy_outlier_stream_is_flagged_noisy() {
        let engine = SequentialEngine::default();
        let mut rng = Rng::new(42);
        let mut accepted: Vec<f64> = Vec::new();
        let mut flags: Vec<bool> = Vec::new();
        for i in 0..400 {
            let x = if i % 4 == 0 {
                100.0 + 500.0 * rng.uniform()
            } else {
                100.0 + 2.0 * rng.normal()
            };
            let outlier = engine.is_outlier(&accepted, x);
            flags.push(outlier);
            if !outlier {
                accepted.push(x);
            }
        }
        assert_eq!(engine.noise_verdict(&flags), SprtDecision::Noisy);
    }
}
