//! Per-iteration setup that is excluded from the timing, and async routines.
//!
//! ```text
//! cargo run --release --example batched_and_async -- --quick
//! ```

use std::future::ready;

use tpt_benchmark::{Bencher, layers, tpt_bench, tpt_bench_group};

#[tpt_bench_group]
#[layers(WallClock)]
mod setup_and_async {
    use super::*;

    // Sorting consumes its input, so each iteration needs a fresh unsorted
    // vector. `iter_batched` builds it outside the timed region.
    #[tpt_bench]
    fn sort_reversed_1000(b: &mut Bencher) {
        b.iter_batched(
            || (0..1000_u32).rev().collect::<Vec<_>>(),
            |mut v| {
                v.sort_unstable();
                v
            },
        );
    }

    // `iter_async` drives each future with a built-in executor. That suits
    // futures that finish without a reactor. With a real runtime, call
    // `iter_async_with(|fut| runtime.block_on(fut), ...)` instead.
    #[tpt_bench]
    fn ready_future(b: &mut Bencher) {
        b.iter_async(|| async { ready(21_u64).await * 2 });
    }
}

tpt_benchmark::main!(setup_and_async);
