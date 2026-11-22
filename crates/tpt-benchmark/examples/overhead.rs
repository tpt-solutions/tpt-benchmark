//! Measures the per-call cost of the clock and the amortised per-iteration
//! overhead of the batched loop. Run with `cargo run --release --example overhead`.

use std::hint::black_box;
use std::time::Instant;

use tpt_benchmark::{Clock, ClockSource};

fn main() {
    for clock in [Clock::best(), Clock::instant()] {
        let source = clock.source();
        // Cost of one clock read pair (start + end), in ns.
        let reads = 2_000_000_u64;
        let t0 = clock.ticks();
        let wall = Instant::now();
        let mut acc = 0_u64;
        for _ in 0..reads {
            acc = acc.wrapping_add(black_box(clock.ticks()));
        }
        black_box(acc);
        let per_read = wall.elapsed().as_nanos() as f64 / reads as f64;
        let _ = t0;
        println!("{source:?}: one clock read ~ {per_read:.2} ns");

        // Batched overhead: one start/end pair around N empty iterations.
        for n in [1_u64, 10, 1_000, 100_000] {
            let batches = 2_000_u64.max(20_000_000 / n);
            let mut total = 0.0;
            for _ in 0..batches {
                let s = clock.ticks();
                for _ in 0..n {
                    black_box(());
                }
                let e = clock.ticks();
                total += clock.ticks_to_ns(e.wrapping_sub(s));
            }
            println!(
                "  N={n:>6}: amortised {:.3} ns per iteration",
                total / (batches * n) as f64
            );
        }
        if source == ClockSource::Instant {
            println!("  (Instant resolution floor applies)");
        }
    }
}
