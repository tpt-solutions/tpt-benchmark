//! Using the engine as a library, with no macros and no command line: call
//! [`measure`] and read the [`Summary`].
//!
//! ```text
//! cargo run --release --example measure_directly
//! ```

use std::time::Duration;

use tpt_benchmark::{Config, StopReason, WallClock, black_box, measure};

fn main() {
    let config = Config {
        warmup: Duration::from_millis(50),
        ..Config::default()
    };
    let summary = measure(&config, WallClock::new(), |b| {
        let data: Vec<u64> = (0..1024).collect();
        b.iter(|| black_box(&data).iter().sum::<u64>());
    });

    println!(
        "mean {:.1} ns/iter, 95% CI {:.1} to {:.1} ns",
        summary.mean, summary.ci_low, summary.ci_high
    );
    println!(
        "{} samples ({} outliers rejected), {} iterations per batch",
        summary.samples, summary.outliers, summary.iters_per_batch
    );
    match summary.stop_reason {
        StopReason::Converged => println!("stopped: the interval met the precision target"),
        other => println!("stopped early at a cap ({other:?}); treat the interval with care"),
    }
    if summary.noisy {
        println!("the run was flagged noisy");
    }
}
