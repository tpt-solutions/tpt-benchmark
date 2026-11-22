//! Demonstrates the attribute macros and `main!` (see spec section 6).
//!
//! Run with `cargo bench -p tpt-benchmark --bench macros_demo -- --quick --noplot`.

use tpt_benchmark::{Bencher, black_box, layers, tpt_bench, tpt_bench_group};

#[tpt_bench_group]
#[layers(WallClock, Allocations, HardwareCounters)]
mod matrix_ops {
    use super::*;

    #[tpt_bench(args = [128, 512])]
    fn dot(b: &mut Bencher, len: usize) {
        let xs: Vec<f64> = (0..len).map(|i| i as f64).collect();
        let ys: Vec<f64> = xs.iter().rev().copied().collect();
        b.iter(|| {
            black_box(&xs)
                .iter()
                .zip(black_box(&ys))
                .map(|(x, y)| x * y)
                .sum::<f64>()
        });
    }

    #[tpt_bench]
    #[layers(WallClock)]
    fn fill(b: &mut Bencher) {
        b.iter(|| black_box(vec![0_u8; 64]));
    }
}

tpt_benchmark::main!(matrix_ops);
