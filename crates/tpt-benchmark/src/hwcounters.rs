//! Hardware performance-counter layers (instructions, cache and branch misses).
//!
//! The common types and the pure helpers live here. Each OS has a backend:
//! Linux uses `perf_event_open` (see `hwcounters/linux.rs`); macOS, Windows and
//! other platforms report [`Unavailable`] (see `hwcounters/unsupported.rs`).
//! Setup and permissions are documented in `docs/hardware-counters.md`.

use crate::layer::{MeasurementLayer, SampleData};

#[cfg(target_os = "linux")]
mod linux;
#[cfg(not(target_os = "linux"))]
mod unsupported;

#[cfg(target_os = "linux")]
use linux::Perf as Backend;
#[cfg(not(target_os = "linux"))]
use unsupported::Never as Backend;

/// A hardware event the layer can count.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counter {
    /// Retired instructions.
    Instructions,
    /// L1 data cache misses (read misses).
    L1Misses,
    /// L2 cache misses. On most CPUs this is the last-level cache (LL) miss
    /// counter, because the generic `perf` events do not expose L2 directly.
    L2Misses,
    /// Mispredicted branches.
    BranchMisses,
}

/// Why a hardware counter cannot be used on this machine.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unavailable {
    /// Human-readable reason, shown in the runner's skip warning.
    pub reason: String,
}

/// One batch's hardware event count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HwSample {
    /// Events counted during the batch, scaled for counter multiplexing.
    pub count: f64,
}

impl SampleData for HwSample {
    fn value(&self) -> f64 {
        self.count
    }
}

/// A hardware counter layer.
///
/// The counter is opened for the thread that calls [`HardwareCounter::try_new`]
/// and counts that thread only. Create the layer on the thread that runs the
/// benchmark batches.
#[derive(Debug)]
pub struct HardwareCounter {
    counter: Counter,
    backend: Backend,
}

impl HardwareCounter {
    /// The event this layer counts.
    #[must_use]
    pub fn counter(&self) -> Counter {
        self.counter
    }

    /// Opens the counter, or explains why it is unavailable.
    ///
    /// Never panics and never prints. The runner decides how to report a
    /// failure.
    ///
    /// # Errors
    ///
    /// Returns [`Unavailable`] when the platform has no backend, the OS denies
    /// access, or the CPU or hypervisor does not expose the event.
    pub fn try_new(counter: Counter) -> Result<Self, Unavailable> {
        let backend = Backend::open(counter)?;
        Ok(Self { counter, backend })
    }
}

impl MeasurementLayer for HardwareCounter {
    type Sample = HwSample;

    fn name(&self) -> &'static str {
        layer_name(self.counter)
    }

    fn start_batch(&mut self) {
        self.backend.start();
    }

    fn end_batch(&mut self) -> HwSample {
        HwSample {
            count: self.backend.end(),
        }
    }
}

/// The layer name for a counter, matching `runner::LayerKind::name`.
const fn layer_name(counter: Counter) -> &'static str {
    match counter {
        Counter::Instructions => "HwInstructions",
        Counter::L1Misses => "HwL1Misses",
        Counter::L2Misses => "HwL2Misses",
        Counter::BranchMisses => "HwBranchMisses",
    }
}

// `perf_event_attr` identifiers from <linux/perf_event.h>. They are plain
// numbers, so the event mapping is testable on every OS.
#[cfg(any(test, target_os = "linux"))]
const PERF_TYPE_HARDWARE: u32 = 0;
#[cfg(any(test, target_os = "linux"))]
const PERF_TYPE_HW_CACHE: u32 = 3;
#[cfg(any(test, target_os = "linux"))]
const PERF_COUNT_HW_INSTRUCTIONS: u64 = 1;
#[cfg(any(test, target_os = "linux"))]
const PERF_COUNT_HW_BRANCH_MISSES: u64 = 5;
#[cfg(any(test, target_os = "linux"))]
const PERF_COUNT_HW_CACHE_L1D: u64 = 0;
#[cfg(any(test, target_os = "linux"))]
const PERF_COUNT_HW_CACHE_LL: u64 = 2;
#[cfg(any(test, target_os = "linux"))]
const PERF_COUNT_HW_CACHE_OP_READ: u64 = 0;
#[cfg(any(test, target_os = "linux"))]
const PERF_COUNT_HW_CACHE_RESULT_MISS: u64 = 1;

/// The perf event type and config for one counter.
#[cfg(any(test, target_os = "linux"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct EventSpec {
    event_type: u32,
    config: u64,
}

/// Packs a `PERF_TYPE_HW_CACHE` config: cache id, operation and result.
#[cfg(any(test, target_os = "linux"))]
const fn cache_config(cache: u64, op: u64, result: u64) -> u64 {
    cache | (op << 8) | (result << 16)
}

/// Maps a counter to its generic perf event. Generic `PERF_TYPE_HARDWARE` and
/// `PERF_TYPE_HW_CACHE` events are the portable subset that Intel, AMD and ARM
/// all expose. Vendor raw events are out of scope for v1.
#[cfg(any(test, target_os = "linux"))]
const fn event_spec(counter: Counter) -> EventSpec {
    match counter {
        Counter::Instructions => EventSpec {
            event_type: PERF_TYPE_HARDWARE,
            config: PERF_COUNT_HW_INSTRUCTIONS,
        },
        Counter::BranchMisses => EventSpec {
            event_type: PERF_TYPE_HARDWARE,
            config: PERF_COUNT_HW_BRANCH_MISSES,
        },
        Counter::L1Misses => EventSpec {
            event_type: PERF_TYPE_HW_CACHE,
            config: cache_config(
                PERF_COUNT_HW_CACHE_L1D,
                PERF_COUNT_HW_CACHE_OP_READ,
                PERF_COUNT_HW_CACHE_RESULT_MISS,
            ),
        },
        Counter::L2Misses => EventSpec {
            event_type: PERF_TYPE_HW_CACHE,
            config: cache_config(
                PERF_COUNT_HW_CACHE_LL,
                PERF_COUNT_HW_CACHE_OP_READ,
                PERF_COUNT_HW_CACHE_RESULT_MISS,
            ),
        },
    }
}

/// Scales a raw count for multiplexing: `value * enabled / running`.
///
/// `enabled` and `running` are the time the event was enabled and actually
/// on a PMU, over the same window. If the event never ran (`running == 0`),
/// the count is unknown and this returns `0.0`.
#[cfg(any(test, target_os = "linux"))]
fn scale_count(value: u64, enabled: u64, running: u64) -> f64 {
    if running == 0 {
        return 0.0;
    }
    value as f64 * enabled as f64 / running as f64
}

/// The user-facing reason when `perf_event_open` fails on Linux.
#[cfg(any(test, target_os = "linux"))]
fn open_failure_reason(counter: Counter, err: &std::io::Error) -> String {
    use std::io::ErrorKind;
    let name = layer_name(counter);
    match err.kind() {
        ErrorKind::PermissionDenied => format!(
            "{name}: perf_event_open was denied ({err}). Lower /proc/sys/kernel/perf_event_paranoid \
             to 2 or less, or grant CAP_PERFMON; see docs/hardware-counters.md"
        ),
        ErrorKind::NotFound | ErrorKind::Unsupported | ErrorKind::InvalidInput => format!(
            "{name}: this CPU or hypervisor does not expose the event ({err}). Virtual machines \
             and some containers hide the PMU; see docs/hardware-counters.md"
        ),
        _ => format!("{name}: perf_event_open failed ({err}); see docs/hardware-counters.md"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn event_specs_match_perf_abi() {
        assert_eq!(
            event_spec(Counter::Instructions),
            EventSpec {
                event_type: 0,
                config: 1
            }
        );
        assert_eq!(
            event_spec(Counter::BranchMisses),
            EventSpec {
                event_type: 0,
                config: 5
            }
        );
        // L1D (0) | READ (0) << 8 | MISS (1) << 16
        assert_eq!(
            event_spec(Counter::L1Misses),
            EventSpec {
                event_type: 3,
                config: 0x1_0000
            }
        );
        // LL (2) | READ (0) << 8 | MISS (1) << 16
        assert_eq!(
            event_spec(Counter::L2Misses),
            EventSpec {
                event_type: 3,
                config: 0x1_0002
            }
        );
    }

    #[test]
    fn layer_names_match_runner_kinds() {
        assert_eq!(layer_name(Counter::Instructions), "HwInstructions");
        assert_eq!(layer_name(Counter::L1Misses), "HwL1Misses");
        assert_eq!(layer_name(Counter::L2Misses), "HwL2Misses");
        assert_eq!(layer_name(Counter::BranchMisses), "HwBranchMisses");
    }

    #[test]
    fn scaling_is_identity_when_always_running() {
        assert_eq!(scale_count(1_000, 500, 500), 1_000.0);
    }

    #[test]
    fn scaling_extrapolates_multiplexed_counts() {
        // Counted for a quarter of the enabled window, so scale by 4.
        assert_eq!(scale_count(250, 1_000, 250), 1_000.0);
        assert_eq!(scale_count(3, 10, 4), 7.5);
    }

    #[test]
    fn scaling_handles_zero_running_time() {
        assert_eq!(scale_count(0, 0, 0), 0.0);
        assert_eq!(scale_count(42, 1_000, 0), 0.0);
    }

    #[test]
    fn permission_reason_names_the_paranoid_knob() {
        let reason = open_failure_reason(
            Counter::Instructions,
            &Error::from(ErrorKind::PermissionDenied),
        );
        assert!(
            reason.contains("/proc/sys/kernel/perf_event_paranoid"),
            "{reason}"
        );
        assert!(reason.contains("HwInstructions"), "{reason}");
        assert!(reason.contains("CAP_PERFMON"), "{reason}");
    }

    #[test]
    fn missing_event_reason_mentions_virtual_machines() {
        let reason = open_failure_reason(Counter::L2Misses, &Error::from(ErrorKind::NotFound));
        assert!(reason.contains("Virtual machines"), "{reason}");
        assert!(reason.contains("HwL2Misses"), "{reason}");
    }

    #[test]
    fn try_new_never_panics_for_any_counter() {
        for counter in [
            Counter::Instructions,
            Counter::L1Misses,
            Counter::L2Misses,
            Counter::BranchMisses,
        ] {
            // Either opens or reports Unavailable; both are acceptable here.
            let unavailable = HardwareCounter::try_new(counter).err();
            if let Some(err) = unavailable {
                assert!(!err.reason.is_empty());
            }
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_reports_the_documented_reason() {
        let err = HardwareCounter::try_new(Counter::Instructions).expect_err("unsupported");
        assert_eq!(
            err.reason,
            "hardware counters not supported on Windows in this build; see docs/hardware-counters.md"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_reports_the_documented_reason() {
        let err = HardwareCounter::try_new(Counter::Instructions).expect_err("unsupported");
        assert_eq!(
            err.reason,
            "kperf (private API, root required) not implemented in this build"
        );
    }

    /// Needs a Linux machine where the user may call `perf_event_open`.
    #[cfg(target_os = "linux")]
    #[test]
    #[ignore = "requires Linux perf access"]
    fn instructions_count_a_known_loop() {
        use std::hint::black_box;
        let mut layer = HardwareCounter::try_new(Counter::Instructions).expect("perf access");
        layer.start_batch();
        let mut x = 0_u64;
        for i in 0..1_000_000_u64 {
            x = black_box(x.wrapping_add(i));
        }
        black_box(x);
        let sample = layer.end_batch();
        assert!(sample.count > 1_000_000.0, "count = {}", sample.count);
    }
}
