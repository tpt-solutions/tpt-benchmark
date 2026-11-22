//! A layered, statistically sequential benchmarking framework.
//!
//! The execution loop is separated from the measurement probes. A benchmark
//! runs under one or more [`MeasurementLayer`]s (wall-clock time today;
//! allocations and hardware counters to follow), and a
//! [`StatisticalEngine`](engine::StatisticalEngine) decides how many samples
//! to take. Sampling stops as soon as the 95 % confidence interval is tight
//! enough, instead of after a fixed count.
//!
//! ```
//! use std::time::Duration;
//! use tpt_benchmark::{black_box, measure, Config, WallClock};
//!
//! let config = Config {
//!     warmup: Duration::from_millis(1),
//!     ..Config::default()
//! };
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
#![warn(missing_docs)]

#[allow(unsafe_code)]
mod clock;

pub mod alloc;
pub mod baseline;
mod bencher;
pub mod engine;
pub mod hwcounters;
mod json;
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
