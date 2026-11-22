//! Procedural macros for `tpt-benchmark`.
//!
//! This crate is an implementation detail: depend on `tpt-benchmark`, which
//! re-exports everything defined here.
//!
//! - `#[tpt_bench]` marks a benchmark function, optionally parameterised with
//!   `args = [...]`.
//! - `#[tpt_bench_group]` on a module collects its `#[tpt_bench]` functions
//!   and generates their registration table, `__tpt_benches()`.
//! - `#[layers(...)]` selects the measurement layers, on a group or on a single
//!   benchmark.
//!
//! Attribute macros cannot see sibling items, so the work happens in
//! `#[tpt_bench_group]`; `#[tpt_bench]` and `#[layers]` on their own only
//! validate their input and leave the item unchanged.

use proc_macro::TokenStream;

mod bench;
mod spec;

use proc_macro2::TokenStream as TokenStream2;

/// Collects compile errors so that several can be reported at once.
#[derive(Default)]
pub(crate) struct Errors(Option<syn::Error>);

impl Errors {
    /// Records an error.
    pub(crate) fn add(&mut self, err: syn::Error) {
        match &mut self.0 {
            Some(existing) => existing.combine(err),
            None => self.0 = Some(err),
        }
    }

    /// Records the error of a failed result, returning the success value.
    pub(crate) fn check<T>(&mut self, result: syn::Result<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(err) => {
                self.add(err);
                None
            }
        }
    }

    /// Whether no error has been recorded.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_none()
    }

    /// Moves every error from `other` into `self`.
    pub(crate) fn absorb(&mut self, other: Errors) {
        if let Some(err) = other.0 {
            self.add(err);
        }
    }

    /// The `compile_error!` tokens for all recorded errors.
    pub(crate) fn into_tokens(self) -> TokenStream2 {
        self.0.map(|err| err.to_compile_error()).unwrap_or_default()
    }
}

/// Marks a benchmark function. Use inside a `#[tpt_bench_group]` module.
///
/// The function takes `&mut tpt_benchmark::Bencher` first, and may take one
/// more parameter when `args = [e1, e2, ...]` is given. Each argument produces
/// one benchmark named `name/<arg>`. The function itself is kept unchanged.
#[proc_macro_attribute]
pub fn tpt_bench(attr: TokenStream, item: TokenStream) -> TokenStream {
    bench::expand_bench(attr.into(), item.into()).into()
}

/// Collects the `#[tpt_bench]` functions of a module into a benchmark group.
///
/// The group is named after the module. The generated
/// `pub fn __tpt_benches() -> Vec<BenchDef>` is what `tpt_benchmark::main!`
/// calls.
#[proc_macro_attribute]
pub fn tpt_bench_group(attr: TokenStream, item: TokenStream) -> TokenStream {
    bench::expand_group(attr.into(), item.into()).into()
}

/// Selects the measurement layers for a group or a single benchmark.
///
/// Accepted names: `WallClock`, `Allocations`, `AllocBytes`,
/// `HardwareCounters` (all four counters), and the individual `HwInstructions`,
/// `HwL1Misses`, `HwL2Misses`, `HwBranchMisses`. Benchmarks without a
/// `#[layers]` attribute run under `WallClock` only.
#[proc_macro_attribute]
pub fn layers(attr: TokenStream, item: TokenStream) -> TokenStream {
    spec::expand_layers(attr.into(), item.into()).into()
}
