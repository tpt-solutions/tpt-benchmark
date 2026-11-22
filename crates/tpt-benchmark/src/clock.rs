//! Raw time sources used by the wall-clock layer.
//!
//! Selection order:
//! - x86/x86_64: `RDTSC`, but only when the CPU advertises an invariant TSC
//!   (CPUID `0x8000_0007`, EDX bit 8) and the calibrated rate is plausible.
//!   The TSC rate is not architecturally exposed, so it is calibrated against
//!   `Instant`.
//! - aarch64: the generic timer counter `CNTVCT_EL0`. Its rate is advertised
//!   in `CNTFRQ_EL0`, and the calibration must agree with it.
//! - Everything else, or any source that fails its checks: [`std::time::Instant`].

use std::time::{Duration, Instant};

/// Which underlying counter a [`Clock`] reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockSource {
    /// x86/x86_64 time-stamp counter.
    Tsc,
    /// aarch64 generic timer counter.
    CntVct,
    /// `std::time::Instant`, nanosecond resolution.
    Instant,
}

/// A monotonic tick source with a calibrated tick-to-nanosecond ratio.
#[derive(Clone, Debug)]
pub struct Clock {
    source: ClockSource,
    ns_per_tick: f64,
    epoch: Instant,
}

impl Clock {
    /// Picks the best available source for this machine.
    #[must_use]
    pub fn best() -> Self {
        hardware_clock().unwrap_or_else(Self::instant)
    }

    /// Forces the portable [`Instant`] source.
    #[must_use]
    pub fn instant() -> Self {
        Self {
            source: ClockSource::Instant,
            ns_per_tick: 1.0,
            epoch: Instant::now(),
        }
    }

    /// The source this clock reads.
    #[must_use]
    pub fn source(&self) -> ClockSource {
        self.source
    }

    /// Current tick count. Only differences between two readings are meaningful.
    #[inline]
    #[must_use]
    pub fn ticks(&self) -> u64 {
        match self.source {
            ClockSource::Instant => {
                u64::try_from(self.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX)
            }
            ClockSource::Tsc | ClockSource::CntVct => raw_ticks(self.source),
        }
    }

    /// Converts a tick difference to nanoseconds.
    #[inline]
    #[must_use]
    pub fn ticks_to_ns(&self, ticks: u64) -> f64 {
        ticks as f64 * self.ns_per_tick
    }
}

/// Attempts the hardware counters; `None` if none is trustworthy here.
fn hardware_clock() -> Option<Clock> {
    #[cfg(any(target_arch = "x86_64", target_arch = "x86"))]
    {
        if invariant_tsc_supported() {
            return calibrate(ClockSource::Tsc, None);
        }
        None
    }
    #[cfg(target_arch = "aarch64")]
    {
        calibrate(ClockSource::CntVct, Some(cntfrq()?))
    }
    #[cfg(not(any(target_arch = "x86_64", target_arch = "x86", target_arch = "aarch64")))]
    {
        None
    }
}

/// Measures the counter against `Instant` over a short window and derives the
/// ns-per-tick ratio. When the hardware advertises a frequency, the measured
/// rate must agree with it to within 5 %; otherwise the source is rejected.
fn calibrate(source: ClockSource, advertised_hz: Option<f64>) -> Option<Clock> {
    let window = Duration::from_millis(10);
    let start_wall = Instant::now();
    let start_ticks = raw_ticks(source);
    while start_wall.elapsed() < window {
        std::hint::spin_loop();
    }
    let elapsed_ns = start_wall.elapsed().as_nanos() as f64;
    let elapsed_ticks = raw_ticks(source).wrapping_sub(start_ticks) as f64;
    if elapsed_ticks <= 0.0 || elapsed_ns <= 0.0 {
        return None;
    }
    let ns_per_tick = elapsed_ns / elapsed_ticks;

    // Plausibility band: 1 GHz to 10 GHz. Anything outside means the counter
    // is not ticking at a CPU-like rate, so it is not safe to use.
    if !(0.1..=1.0).contains(&ns_per_tick) {
        return None;
    }
    if let Some(hz) = advertised_hz {
        let expected_ns_per_tick = 1e9 / hz;
        let drift = (ns_per_tick - expected_ns_per_tick).abs() / expected_ns_per_tick;
        if drift > 0.05 {
            return None;
        }
    }

    Some(Clock {
        source,
        ns_per_tick,
        epoch: Instant::now(),
    })
}

// `__cpuid` is unsafe on the 1.85 MSRV and safe on newer toolchains, so the
// block is kept for both and the lint is allowed here.
#[cfg(target_arch = "x86_64")]
#[allow(unused_unsafe)]
fn invariant_tsc_supported() -> bool {
    use std::arch::x86_64::__cpuid;
    // SAFETY: CPUID exists on every x86_64 CPU. The extended leaf is only read
    // after confirming the CPU reports it.
    unsafe {
        if __cpuid(0x8000_0000).eax < 0x8000_0007 {
            return false;
        }
        (__cpuid(0x8000_0007).edx & (1 << 8)) != 0
    }
}

#[cfg(target_arch = "x86")]
#[allow(unused_unsafe)]
fn invariant_tsc_supported() -> bool {
    use std::arch::x86::__cpuid;
    // SAFETY: as for the x86_64 variant.
    unsafe {
        if __cpuid(0x8000_0000).eax < 0x8000_0007 {
            return false;
        }
        (__cpuid(0x8000_0007).edx & (1 << 8)) != 0
    }
}

#[cfg(target_arch = "aarch64")]
fn cntfrq() -> Option<f64> {
    let freq: u64;
    // SAFETY: reading CNTFRQ_EL0 has no side effects and is readable at EL0
    // on the platforms this crate targets.
    unsafe {
        core::arch::asm!(
            "mrs {}, cntfrq_el0",
            out(reg) freq,
            options(nomem, nostack, preserves_flags)
        );
    }
    (freq != 0).then_some(freq as f64)
}

#[inline]
fn raw_ticks(source: ClockSource) -> u64 {
    match source {
        #[cfg(target_arch = "x86_64")]
        ClockSource::Tsc => {
            // SAFETY: RDTSC has no preconditions. It is only selected after the
            // invariant-TSC check in `hardware_clock`.
            unsafe { std::arch::x86_64::_rdtsc() }
        }
        #[cfg(target_arch = "x86")]
        ClockSource::Tsc => {
            // SAFETY: as above.
            unsafe { std::arch::x86::_rdtsc() }
        }
        #[cfg(target_arch = "aarch64")]
        ClockSource::CntVct => {
            let ticks: u64;
            // SAFETY: reading CNTVCT_EL0 has no side effects.
            unsafe {
                core::arch::asm!(
                    "mrs {}, cntvct_el0",
                    out(reg) ticks,
                    options(nomem, nostack, preserves_flags)
                );
            }
            ticks
        }
        // Unreachable on this target: the source is never selected here.
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instant_clock_is_monotonic() {
        let clock = Clock::instant();
        let a = clock.ticks();
        let b = clock.ticks();
        assert!(b >= a);
        assert_eq!(clock.source(), ClockSource::Instant);
    }

    #[test]
    fn best_clock_converts_a_sleep_to_plausible_ns() {
        let clock = Clock::best();
        let a = clock.ticks();
        std::thread::sleep(Duration::from_millis(5));
        let b = clock.ticks();
        let ns = clock.ticks_to_ns(b.wrapping_sub(a));
        // The sleep is at least 5 ms; the upper bound only catches a
        // calibration that is off by orders of magnitude.
        assert!(ns >= 4_000_000.0, "ns = {ns}");
        assert!(ns < 1_000_000_000.0, "ns = {ns}");
    }
}
