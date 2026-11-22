//! Descriptive statistics for benchmark samples.
//!
//! Everything here works on per-iteration `f64` values (nanoseconds for the
//! wall-clock layer). The mean and variance come from Welford's online
//! algorithm, so they can be updated one sample at a time without storing a
//! running sum that loses precision on long runs.

use serde::{Deserialize, Serialize};

/// Running mean and variance (Welford's algorithm).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Welford {
    count: u64,
    mean: f64,
    m2: f64,
}

impl Welford {
    /// An empty accumulator.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            count: 0,
            mean: 0.0,
            m2: 0.0,
        }
    }

    /// Adds one observation.
    pub fn push(&mut self, x: f64) {
        self.count += 1;
        let delta = x - self.mean;
        self.mean += delta / self.count as f64;
        self.m2 += delta * (x - self.mean);
    }

    /// Number of observations.
    #[must_use]
    pub const fn count(&self) -> u64 {
        self.count
    }

    /// Running mean (0 when empty).
    #[must_use]
    pub const fn mean(&self) -> f64 {
        self.mean
    }

    /// Unbiased sample variance (0 with fewer than two observations).
    #[must_use]
    pub fn variance(&self) -> f64 {
        if self.count < 2 {
            0.0
        } else {
            self.m2 / (self.count - 1) as f64
        }
    }

    /// Sample standard deviation.
    #[must_use]
    pub fn std_dev(&self) -> f64 {
        self.variance().sqrt()
    }

    /// Builds an accumulator from a slice.
    #[must_use]
    pub fn from_slice(values: &[f64]) -> Self {
        let mut w = Self::new();
        for &v in values {
            w.push(v);
        }
        w
    }
}

/// Two-sided 95 % Student-t critical values for 1..=30 degrees of freedom.
const T_95: [f64; 30] = [
    12.706, 4.303, 3.182, 2.776, 2.571, 2.447, 2.365, 2.306, 2.262, 2.228, 2.201, 2.179, 2.160,
    2.145, 2.131, 2.120, 2.110, 2.101, 2.093, 2.086, 2.080, 2.074, 2.069, 2.064, 2.060, 2.056,
    2.052, 2.048, 2.045, 2.042,
];

/// Two-sided 95 % critical value of Student's t with `df` degrees of freedom.
/// Falls back to the normal quantile 1.96 beyond 30 degrees of freedom.
#[must_use]
pub fn t_critical_95(df: u64) -> f64 {
    match df {
        0 => f64::INFINITY,
        1..=30 => T_95[(df - 1) as usize],
        _ => 1.96,
    }
}

/// Half-width of the 95 % confidence interval for the mean.
#[must_use]
pub fn ci_half_width_95(w: &Welford) -> f64 {
    if w.count() < 2 {
        return f64::INFINITY;
    }
    let n = w.count() as f64;
    t_critical_95(w.count() - 1) * (w.variance() / n).sqrt()
}

/// Median of a slice. The slice is copied and sorted, so the input is untouched.
/// Returns `NaN` for an empty slice.
#[must_use]
pub fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return f64::NAN;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}

/// Median absolute deviation scaled to a standard deviation for normal data
/// (the 1.4826 consistency constant). Returns `NaN` for an empty slice.
#[must_use]
pub fn scaled_mad(values: &[f64]) -> f64 {
    let med = median(values);
    if med.is_nan() {
        return f64::NAN;
    }
    let deviations: Vec<f64> = values.iter().map(|v| (v - med).abs()).collect();
    1.4826 * median(&deviations)
}

/// Summary statistics for one measured layer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Summary {
    /// Name of the layer that produced the samples, e.g. `"WallClock"`.
    pub layer: String,
    /// Mean per-iteration value (nanoseconds for the wall-clock layer).
    pub mean: f64,
    /// Sample standard deviation of the accepted per-iteration values.
    pub std_dev: f64,
    /// Median of the accepted per-iteration values.
    pub median: f64,
    /// Lower bound of the 95 % confidence interval for the mean.
    pub ci_low: f64,
    /// Upper bound of the 95 % confidence interval for the mean.
    pub ci_high: f64,
    /// Number of accepted samples.
    pub samples: u64,
    /// The accepted per-iteration values, in arrival order.
    pub values: Vec<f64>,
    /// Number of samples rejected as outliers.
    pub outliers: u64,
    /// Iterations executed per measured batch.
    pub iters_per_batch: u64,
    /// Why sampling stopped.
    pub stop_reason: StopReason,
    /// `true` when the outlier-rate test indicated a noisy environment.
    pub noisy: bool,
    /// Work per iteration, when the benchmark declared it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub throughput: Option<crate::throughput::Throughput>,
}

/// The condition that ended sampling.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum StopReason {
    /// The confidence interval met the relative-error target.
    Converged,
    /// The maximum sample count was reached.
    MaxSamples,
    /// The maximum wall time was reached.
    MaxTime,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welford_matches_two_pass_on_known_data() {
        let data = [2.0, 4.0, 4.0, 4.0, 5.0, 5.0, 7.0, 9.0];
        let w = Welford::from_slice(&data);
        assert_eq!(w.count(), 8);
        assert!((w.mean() - 5.0).abs() < 1e-12);
        // Unbiased variance: sum of squared deviations is 32, over 7.
        assert!((w.variance() - 32.0 / 7.0).abs() < 1e-12);
    }

    #[test]
    fn welford_is_stable_with_large_offset() {
        let base = 1e12;
        let w = Welford::from_slice(&[base + 1.0, base + 2.0, base + 3.0]);
        assert!((w.mean() - (base + 2.0)).abs() < 1e-3);
        assert!((w.variance() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn empty_and_single_have_no_variance() {
        assert_eq!(Welford::new().variance(), 0.0);
        assert_eq!(Welford::from_slice(&[3.0]).variance(), 0.0);
        assert!(ci_half_width_95(&Welford::from_slice(&[3.0])).is_infinite());
    }

    #[test]
    fn t_table_matches_known_values() {
        assert!((t_critical_95(1) - 12.706).abs() < 1e-9);
        assert!((t_critical_95(9) - 2.262).abs() < 1e-9);
        assert!((t_critical_95(30) - 2.042).abs() < 1e-9);
        assert!((t_critical_95(1000) - 1.96).abs() < 1e-9);
    }

    #[test]
    fn ci_shrinks_with_more_samples() {
        let small = Welford::from_slice(&[9.0, 10.0, 11.0, 10.0, 10.0]);
        let mut large = Welford::new();
        for _ in 0..20 {
            for v in [9.0, 10.0, 11.0, 10.0, 10.0] {
                large.push(v);
            }
        }
        assert!(ci_half_width_95(&large) < ci_half_width_95(&small));
    }

    #[test]
    fn median_handles_odd_even_and_empty() {
        assert_eq!(median(&[3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]), 2.5);
        assert!(median(&[]).is_nan());
    }

    #[test]
    fn mad_is_robust_to_a_single_outlier() {
        let clean = [10.0, 10.1, 9.9, 10.0, 10.2, 9.8, 10.0];
        let mut dirty = clean.to_vec();
        dirty.push(1_000.0);
        assert!(scaled_mad(&dirty) < 0.5);
    }
}

/// Deterministic random numbers for property-style and synthetic-distribution tests.
#[cfg(test)]
pub(crate) mod testutil {
    /// xorshift64* generator. Fixed seeds keep every test reproducible.
    #[derive(Clone, Debug)]
    pub(crate) struct Rng(u64);

    impl Rng {
        pub(crate) fn new(seed: u64) -> Self {
            Self(seed.max(1))
        }

        pub(crate) fn next_u64(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        /// Uniform in (0, 1), never exactly 0 or 1.
        pub(crate) fn uniform(&mut self) -> f64 {
            ((self.next_u64() >> 11) as f64 + 0.5) / (1u64 << 53) as f64
        }

        /// Standard normal via Box–Muller.
        pub(crate) fn normal(&mut self) -> f64 {
            let u1 = self.uniform();
            let u2 = self.uniform();
            (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
        }
    }
}

#[cfg(test)]
mod property_tests {
    use super::testutil::Rng;
    use super::*;

    /// Runs `check` for 200 seeded random inputs.
    fn for_seeds(mut check: impl FnMut(&mut Rng)) {
        for seed in 1..=200_u64 {
            let mut rng = Rng::new(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15));
            check(&mut rng);
        }
    }

    fn random_vec(rng: &mut Rng, n: usize) -> Vec<f64> {
        (0..n).map(|_| 1_000.0 + 50.0 * rng.normal()).collect()
    }

    #[test]
    fn welford_agrees_with_two_pass_formula() {
        for_seeds(|rng| {
            let n = 2 + (rng.next_u64() % 500) as usize;
            let xs = random_vec(rng, n);
            let mean = xs.iter().sum::<f64>() / n as f64;
            let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (n as f64 - 1.0);
            let w = Welford::from_slice(&xs);
            assert!((w.mean() - mean).abs() <= 1e-9 * mean.abs().max(1.0));
            assert!((w.variance() - var).abs() <= 1e-9 * var.max(1.0));
        });
    }

    #[test]
    fn median_lies_between_min_and_max() {
        for_seeds(|rng| {
            let n = 1 + (rng.next_u64() % 200) as usize;
            let xs = random_vec(rng, n);
            let m = median(&xs);
            let lo = xs.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            assert!(lo <= m && m <= hi);
        });
    }

    #[test]
    fn mad_is_shift_and_scale_equivariant() {
        for_seeds(|rng| {
            let xs = random_vec(rng, 50);
            let shift = 1e6 * rng.normal();
            let scale = 0.5 + rng.uniform() * 10.0;
            let moved: Vec<f64> = xs.iter().map(|x| x * scale + shift).collect();
            let a = scaled_mad(&xs) * scale;
            let b = scaled_mad(&moved);
            assert!((a - b).abs() <= 1e-6 * b.abs().max(1.0), "{a} vs {b}");
        });
    }

    #[test]
    fn ci_half_width_shrinks_as_one_over_root_n() {
        for_seeds(|rng| {
            let xs = random_vec(rng, 400);
            let small = ci_half_width_95(&Welford::from_slice(&xs[..100]));
            let large = ci_half_width_95(&Welford::from_slice(&xs));
            // Quadrupling the sample size should roughly halve the width.
            let ratio = large / small;
            assert!((0.35..=0.7).contains(&ratio), "ratio = {ratio}");
        });
    }
}
