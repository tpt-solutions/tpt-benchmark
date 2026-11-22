use tpt_benchmark::{Bencher, black_box, layers, tpt_bench, tpt_bench_group};

#[tpt_bench_group]
#[layers(WallClock, AllocBytes, HardwareCounters)]
mod ops {
    use super::*;

    #[tpt_bench(args = [4, 8])]
    fn scale(b: &mut Bencher, n: u64) {
        b.iter(|| black_box(n) * 3);
    }

    #[tpt_bench]
    #[layers(HwL1Misses)]
    fn plain(b: &mut Bencher) {
        b.iter(|| black_box(1_u8));
    }
}

fn main() {
    let defs = ops::__tpt_benches();
    assert_eq!(defs.len(), 3);
}
