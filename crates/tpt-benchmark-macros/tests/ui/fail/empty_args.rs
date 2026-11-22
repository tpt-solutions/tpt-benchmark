use tpt_benchmark::Bencher;

#[tpt_benchmark::tpt_bench_group]
mod ops {
    use super::*;

    #[tpt_benchmark::tpt_bench(args = [])]
    fn plain(b: &mut Bencher, n: u32) {
        b.iter(|| n);
    }
}

fn main() {}
