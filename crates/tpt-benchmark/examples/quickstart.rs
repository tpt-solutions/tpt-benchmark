//! The smallest useful benchmark: one group, one parameterised benchmark.
//!
//! Run it with
//!
//! ```text
//! cargo run --release --example quickstart -- --quick
//! ```
//!
//! Drop `--quick` for a full-precision run. `--list` shows the benchmark names
//! and a name on the command line (`... -- sum/1024`) selects one.

use tpt_benchmark::{Bencher, Throughput, black_box, layers, tpt_bench, tpt_bench_group};

#[tpt_bench_group]
#[layers(WallClock)]
mod sums {
    use super::*;

    // `args` runs the function once per value and names the results `sum/128`
    // and `sum/1024`.
    #[tpt_bench(args = [128, 1024])]
    fn sum(b: &mut Bencher, n: usize) {
        // Setup outside `b.iter` is not timed.
        let data: Vec<u64> = (0..n as u64).collect();
        // Also record a rate (elements per second) in the summary and the JSON export.
        b.throughput(Throughput::Elements(n as u64));
        // `black_box` stops the compiler from deleting the work.
        b.iter(|| black_box(&data).iter().sum::<u64>());
    }
}

tpt_benchmark::main!(sums);
