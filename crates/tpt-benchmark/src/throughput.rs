//! Throughput units: how much work one iteration represents.

use serde::{Deserialize, Serialize};

/// The amount of work done by one iteration of a benchmark.
///
/// Set it with [`Bencher::throughput`](crate::Bencher::throughput). The summary
/// then reports a rate per second alongside the time per iteration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "snake_case")]
pub enum Throughput {
    /// Bytes processed per iteration.
    Bytes(u64),
    /// Logical elements processed per iteration.
    Elements(u64),
}

impl Throughput {
    /// The per-iteration quantity, whichever unit is used.
    #[must_use]
    pub const fn per_iter(self) -> u64 {
        match self {
            Self::Bytes(n) | Self::Elements(n) => n,
        }
    }

    /// Converts a mean time per iteration (nanoseconds) to units per second.
    /// Returns `None` for a zero or non-finite time.
    #[must_use]
    pub fn rate_per_second(self, mean_ns: f64) -> Option<f64> {
        if !mean_ns.is_finite() || mean_ns <= 0.0 {
            return None;
        }
        Some(self.per_iter() as f64 * 1e9 / mean_ns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_converts_ns_per_iter_to_units_per_second() {
        // 1000 bytes in 1 µs is 1e9 bytes per second.
        let rate = Throughput::Bytes(1000).rate_per_second(1000.0).unwrap();
        assert!((rate - 1e9).abs() < 1e-3);
    }

    #[test]
    fn rate_is_none_for_degenerate_times() {
        assert!(Throughput::Elements(1).rate_per_second(0.0).is_none());
        assert!(Throughput::Elements(1).rate_per_second(f64::NAN).is_none());
    }
}
