//! Stress tests for the sequential engine: a very fast routine, a very slow
//! routine and a noisy routine. Each must terminate and produce a sane summary.
//!
//! Time budgets are kept small (`max_time` of 2 s) so the suite stays well
//! under half a minute. The assertions cover termination, the stop reason and
//! the shape of the summary, plus generous wall-clock bounds. No mean is
//! compared against a fixed value.

use std::hint::black_box;
use std::time::{Duration, Instant};

use tpt_benchmark::engine::SequentialEngine;
use tpt_benchmark::{Clock, Config, StopReason, Summary, WallClock, measure};

/// A configuration with a short warm-up and batch calibration target, and the
/// given sampling time cap.
fn stress_config(max_time: Duration) -> Config {
    Config {
        warmup: Duration::from_millis(5),
        min_batch: Duration::from_millis(1),
        max_batch_iters: 1 << 24,
        engine: SequentialEngine {
            max_time,
            ..SequentialEngine::default()
        },
    }
}

/// A small deterministic generator (xorshift64*) for the noisy routine.
struct XorShift(u64);

impl XorShift {
    fn next_u64(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
}

/// (a) A routine far below 1 ns per iteration. The batch calibration has to
/// grow the iteration count a long way, and the summary must still be finite
/// and positive without the run hanging.
#[test]
fn very_fast_routine_gives_finite_positive_summary() {
    let config = stress_config(Duration::from_secs(1));
    let started = Instant::now();
    let mut acc: u64 = 0;
    let summary: Summary = measure(&config, WallClock::new(), |b| {
        b.iter(|| {
            acc = acc.wrapping_add(black_box(1_u64));
            acc
        });
    });
    let elapsed = started.elapsed();

    assert!(summary.mean.is_finite(), "mean = {}", summary.mean);
    assert!(summary.mean > 0.0, "mean = {}", summary.mean);
    assert!(
        summary.samples >= config.engine.min_samples,
        "samples = {}",
        summary.samples
    );
    assert!(
        elapsed < Duration::from_secs(10),
        "fast routine took {elapsed:?}"
    );
    eprintln!(
        "fast: mean = {:.4} ns, samples = {}, iters/batch = {}, stop = {:?}, elapsed = {:?}",
        summary.mean, summary.samples, summary.iters_per_batch, summary.stop_reason, elapsed
    );
}

/// (b) A routine of about 50 ms per iteration. The sampling loop must stop at
/// the `max_time` cap, not after a fixed count, and terminate within the cap
/// plus one batch.
///
/// With a 10-iteration calibration batch (about 0.5 s), at most a handful of
/// batches fit in a 2 s cap, so the engine cannot reach `min_samples` (10).
/// The only reachable stop is therefore `MaxTime`.
#[test]
fn very_slow_routine_stops_at_time_cap() {
    let max_time = Duration::from_secs(2);
    let config = stress_config(max_time);
    // Calibration batch of 10 iterations is about 0.5 s; one batch of
    // headroom on top of the cap is about 0.5 s. Allow generous slack for
    // scheduler jitter.
    let budget = max_time + Duration::from_secs(3);

    let started = Instant::now();
    let summary = measure(&config, WallClock::new(), |b| {
        b.iter(|| std::thread::sleep(Duration::from_millis(50)));
    });
    let elapsed = started.elapsed();

    assert!(elapsed < budget, "slow routine took {elapsed:?}");
    assert_eq!(
        summary.stop_reason,
        StopReason::MaxTime,
        "stop reason = {:?}, samples = {}",
        summary.stop_reason,
        summary.samples
    );
    assert!(summary.mean.is_finite() && summary.mean > 0.0);
    eprintln!(
        "slow: mean = {:.3} ms, samples = {}, iters/batch = {}, stop = {:?}, elapsed = {:?}",
        summary.mean / 1e6,
        summary.samples,
        summary.iters_per_batch,
        summary.stop_reason,
        elapsed
    );
}

/// (c) A routine that sleeps for a random 0 to 2 ms per iteration, drawn from
/// a fixed seed. The run must end with a plausible mean. If it reports
/// convergence, the interval must really meet the target precision.
///
/// Whether it converges depends on how batch calibration lands, so the test
/// accepts a noisy flag, a cap, or an honest convergence.
#[test]
fn noisy_routine_terminates_and_is_flagged_or_capped() {
    let config = stress_config(Duration::from_secs(2));
    let mut rng = XorShift(0x5EED_1234_ABCD_0001);

    let started = Instant::now();
    let summary = measure(&config, WallClock::with_clock(Clock::best()), |b| {
        b.iter(|| {
            let micros = rng.next_u64() % 2_000;
            std::thread::sleep(Duration::from_micros(micros));
        });
    });
    let elapsed = started.elapsed();

    assert!(
        elapsed < Duration::from_secs(20),
        "noisy routine took {elapsed:?}"
    );
    assert!(summary.mean.is_finite() && summary.mean > 0.0);
    // Each sample is the mean of a whole batch, so per-iteration noise is
    // averaged out and an honest run can converge on this routine without the
    // noisy flag (it did on the Windows CI runner, where batch calibration
    // landed on larger batches). What must never happen is a converged
    // result whose interval is wider than the precision it claims.
    if summary.stop_reason == StopReason::Converged && !summary.noisy {
        let half_width = (summary.ci_high - summary.ci_low) / 2.0;
        let relative = half_width / summary.mean.abs();
        assert!(
            relative <= config.engine.rel_error * (1.0 + 1e-9),
            "converged with a {:.2}% half-width, target {:.2}%: samples = {}, mean = {}",
            relative * 100.0,
            config.engine.rel_error * 100.0,
            summary.samples,
            summary.mean
        );
    }
    // The routine sleeps 1 ms on average and sleeps never return early, so the
    // mean cannot be much below 1 ms. The upper bound only catches gross errors.
    assert!(
        summary.mean > 0.9e6 && summary.mean < 10e6,
        "implausible mean for a 0-2 ms random sleep: {} ns",
        summary.mean
    );
    eprintln!(
        "noisy: mean = {:.3} ms, samples = {}, outliers = {}, noisy = {}, stop = {:?}, elapsed = {:?}",
        summary.mean / 1e6,
        summary.samples,
        summary.outliers,
        summary.noisy,
        summary.stop_reason,
        elapsed
    );
}
