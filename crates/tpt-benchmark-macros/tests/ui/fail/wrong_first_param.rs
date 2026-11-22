#[tpt_benchmark::tpt_bench_group]
mod ops {
    #[tpt_benchmark::tpt_bench]
    fn plain(count: u32) {
        let _ = count;
    }
}

fn main() {}
