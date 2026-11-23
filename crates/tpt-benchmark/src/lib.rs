//! A layered, statistically sequential benchmarking framework.
//!
//! The execution loop is separated from the measurement probes. A benchmark
//! runs under one or more [`MeasurementLayer`]s (wall-clock time, heap
//! allocations, and hardware counters on Linux), and a
//! [`StatisticalEngine`](engine::StatisticalEngine) decides how many samples
//! to take. Sampling stops as soon as the 95 % confidence interval is tight
//! enough, instead of after a fixed count.
//!
//! # Where to start
//!
//! - For benchmarks you run with `cargo bench`, use the attribute macros
//!   [`tpt_bench`], [`tpt_bench_group`] and [`layers`], with [`main!`] as the
//!   entry point of a `harness = false` bench target. The `examples/`
//!   directory has runnable versions.
//! - To measure one routine from your own code, call [`measure`].
//! - To count allocations, install [`CountingAllocator`] as the global
//!   allocator and select the `Allocations` layer.
//!
//! The Markdown guides in `docs/` cover the statistics, the layers, the JSON
//! export and migrating from Criterion.
//!
//! # Example
//!
//! ```
//! use std::time::Duration;
//! use tpt_benchmark::{black_box, measure, Config, WallClock};
//!
//! let config = Config::default().with_warmup(Duration::from_millis(1));
//! let summary = measure(&config, WallClock::new(), |b| {
//!     b.iter(|| black_box(3_u64).wrapping_mul(7));
//! });
//! assert!(summary.samples >= config.engine.min_samples);
//! ```
//!
//! # Unsafe code
//!
//! The crate forbids `unsafe` everywhere except the clock module, which uses
//! CPU counter intrinsics (`RDTSC`, `CNTVCT_EL0`). Each block there states its
//! invariant.

#![deny(unsafe_code)]
#![warn(missing_debug_implementations)]
#![deny(missing_docs)]

/// Generates consuming `with_*` setters, so a `#[non_exhaustive]` settings
/// struct can still be configured in one expression.
macro_rules! setters {
    ($($(#[$meta:meta])* $name:ident($field:ident: $ty:ty);)*) => {
        $(
            $(#[$meta])*
            #[must_use]
            pub fn $name(mut self, $field: $ty) -> Self {
                self.$field = $field;
                self
            }
        )*
    };
}

#[allow(unsafe_code)]
mod clock;

pub mod alloc;
pub mod baseline;
mod bencher;
pub mod engine;
pub mod environment;
pub mod hwcounters;
pub mod json;
mod layer;
mod macros;
pub mod report;
pub mod runner;
mod stats;
mod throughput;
mod wallclock;

pub use std::hint::black_box;

pub use alloc::{
    AllocBytes, AllocBytesSample, AllocSample, Allocations, CountingAllocator,
    thread_dealloc_totals,
};
pub use bencher::{Bencher, Config, measure};
pub use clock::{Clock, ClockSource};
pub use hwcounters::{Counter, HardwareCounter, HwSample, Unavailable};
pub use layer::{MeasurementLayer, SampleData};
pub use runner::{BenchDef, BenchResult, LayerKind};
pub use stats::{StopReason, Summary, Welford};
pub use throughput::Throughput;
pub use tpt_benchmark_macros::{layers, tpt_bench, tpt_bench_group};
pub use wallclock::{WallClock, WallSample};
