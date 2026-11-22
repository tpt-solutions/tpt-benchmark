use tpt_benchmark::Bencher;

#[tpt_benchmark::tpt_bench_group]
#[tpt_benchmark::layers(WallClock, Bogus)]
mod ops {
    use super::*;

    #[tpt_benchmark::tpt_bench]
    fn plain(b: &mut Bencher) {
        b.iter(|| 1_u8);
    }
}

fn main() {}
