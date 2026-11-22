//! Linux backend: one `perf_event_open` counter per layer, for the calling thread.
//!
//! The counter is opened enabled and kept running. `start_batch` and
//! `end_batch` read it and subtract, rather than resetting it with ioctls.
//! That keeps the counter's history intact and avoids disturbing other readers
//! of the same thread. Each read returns `value, time_enabled, time_running`,
//! so the delta can be scaled for multiplexing.
//!
//! `exclude_kernel` and `exclude_hv` are always set. This counts user-space
//! instructions only, which is what the benchmarked code is. It also keeps the
//! counter usable at `perf_event_paranoid = 2`, the default on most distros.

#![allow(unsafe_code)]

use std::io;
use std::mem::size_of;
use std::os::raw::{c_int, c_ulong, c_void};

use super::{Counter, EventSpec, Unavailable, event_spec, open_failure_reason, scale_count};

/// Mirrors `struct perf_event_attr` through `PERF_ATTR_SIZE_VER5` (112 bytes).
/// Fields we do not set stay zero.
#[repr(C)]
#[derive(Default)]
struct PerfEventAttr {
    event_type: u32,
    size: u32,
    config: u64,
    sample_period_or_freq: u64,
    sample_type: u64,
    read_format: u64,
    /// Bitfield: `exclude_kernel` is bit 5, `exclude_hv` is bit 6.
    flags: u64,
    wakeup_events_or_watermark: u32,
    bp_type: u32,
    config1: u64,
    config2: u64,
    branch_sample_type: u64,
    sample_regs_user: u64,
    sample_stack_user: u32,
    clockid: i32,
    sample_regs_intr: u64,
    aux_watermark: u32,
    sample_max_stack: u16,
    reserved: u16,
}

const FLAG_EXCLUDE_KERNEL: u64 = 1 << 5;
const FLAG_EXCLUDE_HV: u64 = 1 << 6;
const FORMAT_TOTAL_TIME_ENABLED: u64 = 1 << 0;
const FORMAT_TOTAL_TIME_RUNNING: u64 = 1 << 1;
/// `PERF_FLAG_FD_CLOEXEC`: the fd is closed on `exec`.
const FLAG_FD_CLOEXEC: c_ulong = 1 << 3;

/// One read of the counter: the raw count and the times it was enabled and
/// actually running on a PMU.
#[derive(Clone, Copy, Debug, Default)]
struct Reading {
    value: u64,
    enabled: u64,
    running: u64,
}

/// An open perf counter for the thread that created it.
///
/// It must be used on that thread: the counter follows the thread that
/// opened it.
#[derive(Debug)]
pub(crate) struct Perf {
    fd: c_int,
    start: Reading,
}

impl Perf {
    /// Opens a counter for the calling thread, or explains why it cannot.
    pub(crate) fn open(counter: Counter) -> Result<Self, Unavailable> {
        let EventSpec { event_type, config } = event_spec(counter);
        let attr = PerfEventAttr {
            event_type,
            size: size_of::<PerfEventAttr>() as u32,
            config,
            read_format: FORMAT_TOTAL_TIME_ENABLED | FORMAT_TOTAL_TIME_RUNNING,
            flags: FLAG_EXCLUDE_KERNEL | FLAG_EXCLUDE_HV,
            ..PerfEventAttr::default()
        };

        // SAFETY: `attr` is a fully initialised, `repr(C)` `perf_event_attr`
        // whose `size` field equals its size, as the kernel requires. The
        // arguments are pid 0 (this thread), cpu -1 (any CPU), group -1 (no
        // group) and the CLOEXEC flag, all valid values. The kernel reads
        // `attr` only during the call, so the reference outlives the syscall.
        let ret = unsafe {
            libc::syscall(
                libc::SYS_perf_event_open,
                &attr as *const PerfEventAttr,
                0 as c_int,
                -1 as c_int,
                -1 as c_int,
                FLAG_FD_CLOEXEC,
            )
        };
        if ret < 0 {
            let err = io::Error::last_os_error();
            return Err(Unavailable {
                reason: open_failure_reason(counter, &err),
            });
        }
        let fd = c_int::try_from(ret).map_err(|_| Unavailable {
            reason: format!("perf_event_open returned an invalid fd for {counter:?}"),
        })?;
        Ok(Self {
            fd,
            start: Reading::default(),
        })
    }

    /// Records the counter's current value as the batch's starting point.
    pub(crate) fn start(&mut self) {
        self.start = self.read_now().unwrap_or_default();
    }

    /// Returns the events counted since `start`, scaled for multiplexing.
    /// A failed read reports `0.0` for this batch.
    pub(crate) fn end(&mut self) -> f64 {
        let Some(now) = self.read_now() else {
            return 0.0;
        };
        scale_count(
            now.value.wrapping_sub(self.start.value),
            now.enabled.wrapping_sub(self.start.enabled),
            now.running.wrapping_sub(self.start.running),
        )
    }

    fn read_now(&self) -> Option<Reading> {
        let mut buf = [0_u64; 3];
        let want = size_of::<[u64; 3]>();
        // SAFETY: `buf` is 24 writable bytes and we pass exactly that length,
        // so the kernel writes only inside `buf`. `self.fd` is an open perf
        // fd that this struct owns and has not closed.
        let n = unsafe { libc::read(self.fd, buf.as_mut_ptr().cast::<c_void>(), want) };
        if n != want as isize {
            return None;
        }
        Some(Reading {
            value: buf[0],
            enabled: buf[1],
            running: buf[2],
        })
    }
}

impl Drop for Perf {
    fn drop(&mut self) {
        // SAFETY: `self.fd` came from a successful `perf_event_open` and is
        // closed only here, once, because `drop` runs once.
        unsafe {
            libc::close(self.fd);
        }
    }
}
