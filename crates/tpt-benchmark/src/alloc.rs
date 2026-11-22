//! Allocation-counting layers and the counting global allocator.
//!
//! [`Allocations`] and [`AllocBytes`] report the heap calls and bytes made by
//! the calling thread between `start_batch` and `end_batch`. They need the
//! [`CountingAllocator`] installed as the global allocator of the binary;
//! see `docs/allocations.md`.

mod counting;

use std::hint::black_box;

pub use counting::CountingAllocator;

/// The calling thread's cumulative deallocation totals, as `(calls, bytes)`.
///
/// Counts since the thread started, not per batch. Returns zeros when the
/// counting allocator is not installed.
#[must_use]
pub fn thread_dealloc_totals() -> (u64, u64) {
    counting::thread_dealloc_totals()
}

use crate::hwcounters::Unavailable;
use crate::layer::{MeasurementLayer, SampleData};

const NOT_INSTALLED: &str = "counting allocator is not installed; add \
     `#[global_allocator] static ALLOC: tpt_benchmark::alloc::CountingAllocator = \
     tpt_benchmark::alloc::CountingAllocator::new();` (see docs/allocations.md)";

/// Allocation call count for one batch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AllocSample {
    /// Allocation calls in the batch (`alloc`, `alloc_zeroed` and `realloc`).
    pub count: f64,
}

impl SampleData for AllocSample {
    fn value(&self) -> f64 {
        self.count
    }
}

/// Bytes requested for one batch.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AllocBytesSample {
    /// Bytes requested in the batch. `realloc` counts its new size, so this
    /// is gross bytes requested, not net bytes retained.
    pub bytes: f64,
}

impl SampleData for AllocBytesSample {
    fn value(&self) -> f64 {
        self.bytes
    }
}

/// Whether the counting allocator is the process's global allocator.
///
/// Probes by making one allocation on the calling thread and checking that
/// the thread's counter moved.
fn allocator_installed() -> bool {
    let before = counting::thread_totals().calls;
    let probe = black_box(Box::new(0_u8));
    drop(black_box(probe));
    counting::thread_totals().calls > before
}

/// Counts heap allocation calls made by the calling thread in each batch.
///
/// Use [`Allocations::try_new`] to find out whether the counting allocator is
/// installed. [`Allocations::new`] does not check; if the allocator is
/// missing, every batch reports zero.
///
/// Only the thread that calls `start_batch`/`end_batch` is counted. Allocations
/// on other threads are ignored.
#[derive(Clone, Copy, Debug, Default)]
pub struct Allocations {
    start: u64,
}

impl Allocations {
    /// Creates the layer without checking that the allocator is installed.
    #[must_use]
    pub const fn new() -> Self {
        Self { start: 0 }
    }

    /// Creates the layer, or reports that the counting allocator is not the
    /// global allocator of this binary.
    ///
    /// # Errors
    ///
    /// Returns [`Unavailable`] if a probe allocation was not counted.
    pub fn try_new() -> Result<Self, Unavailable> {
        check_installed()?;
        Ok(Self::new())
    }
}

impl MeasurementLayer for Allocations {
    type Sample = AllocSample;

    fn name(&self) -> &'static str {
        "Allocations"
    }

    fn start_batch(&mut self) {
        self.start = counting::thread_totals().calls;
    }

    fn end_batch(&mut self) -> AllocSample {
        let now = counting::thread_totals().calls;
        AllocSample {
            count: now.wrapping_sub(self.start) as f64,
        }
    }
}

/// Counts heap bytes requested by the calling thread in each batch.
///
/// Like [`Allocations`], it needs the [`CountingAllocator`] installed, and it
/// counts only the calling thread.
#[derive(Clone, Copy, Debug, Default)]
pub struct AllocBytes {
    start: u64,
}

impl AllocBytes {
    /// Creates the layer without checking that the allocator is installed.
    #[must_use]
    pub const fn new() -> Self {
        Self { start: 0 }
    }

    /// Creates the layer, or reports that the counting allocator is not the
    /// global allocator of this binary.
    ///
    /// # Errors
    ///
    /// Returns [`Unavailable`] if a probe allocation was not counted.
    pub fn try_new() -> Result<Self, Unavailable> {
        check_installed()?;
        Ok(Self::new())
    }
}

impl MeasurementLayer for AllocBytes {
    type Sample = AllocBytesSample;

    fn name(&self) -> &'static str {
        "AllocBytes"
    }

    fn start_batch(&mut self) {
        self.start = counting::thread_totals().bytes;
    }

    fn end_batch(&mut self) -> AllocBytesSample {
        let now = counting::thread_totals().bytes;
        AllocBytesSample {
            bytes: now.wrapping_sub(self.start) as f64,
        }
    }
}

fn check_installed() -> Result<(), Unavailable> {
    if allocator_installed() {
        Ok(())
    } else {
        Err(Unavailable {
            reason: NOT_INSTALLED.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The lib's unit-test binary does not install the counting allocator, so
    // these tests check the "not installed" path. Counting tests live in
    // tests/alloc_counting.rs, which installs it.

    #[test]
    fn try_new_reports_unavailable_when_not_installed() {
        let err = Allocations::try_new().expect_err("allocator should not be installed");
        assert!(err.reason.contains("#[global_allocator]"), "{}", err.reason);
        assert!(AllocBytes::try_new().is_err());
    }

    #[test]
    fn new_reports_zero_when_not_installed() {
        let mut layer = Allocations::new();
        layer.start_batch();
        black_box(Box::new(1_u64));
        assert_eq!(layer.end_batch().count, 0.0);
    }

    #[test]
    fn layer_names() {
        assert_eq!(Allocations::new().name(), "Allocations");
        assert_eq!(AllocBytes::new().name(), "AllocBytes");
    }
}
