//! The wall-clock measurement layer.

use crate::clock::Clock;
use crate::layer::{MeasurementLayer, SampleData};

/// Elapsed wall time for one batch, in nanoseconds.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub struct WallSample {
    /// Total nanoseconds for the batch.
    pub ns: f64,
}

impl SampleData for WallSample {
    fn value(&self) -> f64 {
        self.ns
    }
}

/// Measures elapsed time using the fastest trustworthy clock on this machine.
#[derive(Clone, Debug)]
pub struct WallClock {
    clock: Clock,
    start: u64,
}

impl WallClock {
    /// Uses the best available clock (see the crate's clock selection notes).
    #[must_use]
    pub fn new() -> Self {
        Self::with_clock(Clock::best())
    }

    /// Uses a specific clock, e.g. [`Clock::instant`] to force the portable path.
    #[must_use]
    pub fn with_clock(clock: Clock) -> Self {
        Self { clock, start: 0 }
    }

    /// The clock this layer reads.
    #[must_use]
    pub fn clock(&self) -> &Clock {
        &self.clock
    }
}

impl Default for WallClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MeasurementLayer for WallClock {
    type Sample = WallSample;

    fn name(&self) -> &'static str {
        "WallClock"
    }

    fn start_batch(&mut self) {
        self.start = self.clock.ticks();
    }

    fn end_batch(&mut self) -> WallSample {
        let elapsed = self.clock.ticks().wrapping_sub(self.start);
        WallSample {
            ns: self.clock.ticks_to_ns(elapsed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_measures_a_known_sleep() {
        let mut layer = WallClock::default();
        layer.start_batch();
        std::thread::sleep(std::time::Duration::from_millis(10));
        let sample = layer.end_batch();
        assert!(sample.ns >= 9_000_000.0, "ns = {}", sample.ns);
    }

    #[test]
    fn instant_fallback_is_usable() {
        let mut layer = WallClock::with_clock(Clock::instant());
        layer.start_batch();
        let sample = layer.end_batch();
        assert!(sample.ns >= 0.0);
        assert_eq!(layer.name(), "WallClock");
    }
}
