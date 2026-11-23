//! Counting heap allocations next to time, and proving a function allocates
//! nothing.
//!
//! The allocation layers only work when the counting allocator is the
//! program's global allocator, which the library cannot install for you.
//!
//! ```text
//! cargo run --release --example allocations -- --quick
//! ```
//!
//! `without_alloc` reports exactly 0 allocations per iteration. `with_alloc`
//! reports 1 allocation of 4096 bytes.

use tpt_benchmark::{Bencher, CountingAllocator, black_box, layers, tpt_bench, tpt_bench_group};

#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

#[tpt_bench_group]
#[layers(WallClock, Allocations, AllocBytes)]
mod alloc_demo {
    use super::*;

    #[tpt_bench]
    fn without_alloc(b: &mut Bencher) {
        let data = [1_u64, 2, 3, 4];
        b.iter(|| black_box(&data).iter().map(|x| x * 2).sum::<u64>());
    }

    #[tpt_bench]
    fn with_alloc(b: &mut Bencher) {
        b.iter(|| vec![0_u8; black_box(4096)]);
    }
}

tpt_benchmark::main!(alloc_demo);
