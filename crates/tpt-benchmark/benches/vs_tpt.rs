//! tpt-benchmark side of the tpt-benchmark vs Criterion comparison.
//!
//! Run with `cargo bench -p tpt-benchmark --bench vs_tpt`. The WallClock
//! summaries land in `target/tpt-benchmark/tpt-benchmark.json`. The workloads
//! are shared with `vs_criterion` through `common/workloads.rs`.

#[path = "common/workloads.rs"]
mod workloads;

use tpt_benchmark::{Bencher, black_box, layers, tpt_bench, tpt_bench_group};

#[tpt_bench_group]
#[layers(WallClock)]
mod vs_tpt {
    use super::*;

    #[tpt_bench]
    fn sum_u64_1024(b: &mut Bencher) {
        let sum_input = workloads::sum_input();
        b.iter(|| workloads::sum_u64(&sum_input));
    }

    #[tpt_bench]
    fn dot_f64_128x128(b: &mut Bencher) {
        let (a, m) = workloads::dot_input();
        b.iter(|| workloads::dot_f64(&a, &m));
    }

    #[tpt_bench]
    fn format_string_100(b: &mut Bencher) {
        b.iter(workloads::build_string);
    }

    #[tpt_bench]
    fn fast_mul_u64(b: &mut Bencher) {
        b.iter(|| workloads::fast_mul(black_box(workloads::FAST_INPUT)));
    }
}

tpt_benchmark::main!(vs_tpt);
