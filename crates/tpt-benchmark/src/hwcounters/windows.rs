//! Windows backend (EXPERIMENTAL): ETW PMC sources plus thread profiling.
//!
//! **Status.** Only the first step is verified. On the development machine
//! (Windows 11 with Hyper-V and virtualization-based security on) Windows answers
//! the PMC source query with `ERROR_NOT_SUPPORTED`, with or without elevation, so
//! the layer reports itself unavailable there. That path is tested. Everything
//! after it, starting the kernel trace session, selecting the counter and reading
//! it back, follows Microsoft's documented sequence but has never run against
//! real counters. Every failure becomes [`Unavailable`], and nothing here panics.
//! See `docs/hardware-counters.md`.
//!
//! **Sequence.**
//! 1. Enable `SeSystemProfilePrivilege` on the process token (needs administrator).
//! 2. Ask ETW which PMC "profile sources" this CPU offers
//!    (`TraceQueryInformation`, `TraceProfileSourceListInfo`) and pick the one
//!    for the requested event by name.
//! 3. Start the NT Kernel Logger session and give it that source as its PMC
//!    counter list (`TraceSetInformation`, `TracePmcCounterListInfo`).
//! 4. Enable thread profiling with that counter on the calling thread
//!    (`EnableThreadProfiling`) and read it with `ReadThreadProfilingData`,
//!    which `start` and `end` subtract.
//!
//! Starting the kernel logger is system-wide: it fails if another tool already
//! owns it, and it is stopped again when the counter is dropped.

#![allow(unsafe_code)]

use std::ffi::c_void;

use super::{Counter, Unavailable, layer_name, pick_first_available, windows_source_names};

type Handle = *mut c_void;

const ERROR_ACCESS_DENIED: u32 = 5;
const ERROR_NOT_SUPPORTED: u32 = 50;
const ERROR_ALREADY_EXISTS: u32 = 183;
const ERROR_NOT_ALL_ASSIGNED: u32 = 1300;

const TOKEN_ADJUST_PRIVILEGES: u32 = 0x0020;
const TOKEN_QUERY: u32 = 0x0008;
const SE_PRIVILEGE_ENABLED: u32 = 0x0000_0002;

const TRACE_PROFILE_SOURCE_LIST_INFO: u32 = 8;
const TRACE_PMC_COUNTER_LIST_INFO: u32 = 10;
const EVENT_TRACE_CONTROL_STOP: u32 = 1;
const EVENT_TRACE_REAL_TIME_MODE: u32 = 0x0000_0100;
const WNODE_FLAG_TRACED_GUID: u32 = 0x0002_0000;
const THREAD_PROFILING_FLAG_DISPATCH: u32 = 1;
const READ_THREAD_PROFILING_FLAG_HARDWARE_COUNTERS: u32 = 2;
const MAX_HW_COUNTERS: usize = 16;

/// `SystemTraceControlGuid`, the provider GUID of the NT Kernel Logger.
const SYSTEM_TRACE_CONTROL_GUID: Guid = Guid {
    data1: 0x9e81_4aad,
    data2: 0x3204,
    data3: 0x11d2,
    data4: [0x9a, 0x82, 0x00, 0x60, 0x08, 0xa8, 0x69, 0x39],
};

#[repr(C)]
#[derive(Clone, Copy)]
struct Guid {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

/// `EVENT_TRACE_PROPERTIES` (x64 layout, 120 bytes), followed in memory by the
/// session name.
#[repr(C)]
struct EventTraceProperties {
    // WNODE_HEADER
    wnode_buffer_size: u32,
    provider_id: u32,
    historical_context: u64,
    kernel_handle_or_timestamp: u64,
    guid: Guid,
    client_context: u32,
    wnode_flags: u32,
    // The rest of the structure.
    buffer_size: u32,
    minimum_buffers: u32,
    maximum_buffers: u32,
    maximum_file_size: u32,
    log_file_mode: u32,
    flush_timer: u32,
    enable_flags: u32,
    age_limit: i32,
    number_of_buffers: u32,
    free_buffers: u32,
    events_lost: u32,
    buffers_written: u32,
    log_buffers_lost: u32,
    real_time_buffers_lost: u32,
    logger_thread_id: u64,
    log_file_name_offset: u32,
    logger_name_offset: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct HardwareCounterData {
    kind: u32,
    reserved: u32,
    value: u64,
}

/// `PERFORMANCE_DATA`.
#[repr(C)]
struct PerformanceData {
    size: u16,
    version: u8,
    hw_counters_count: u8,
    context_switch_count: u32,
    wait_reason_bitmap: u64,
    cycle_time: u64,
    retry_count: u32,
    reserved: u32,
    hw: [HardwareCounterData; MAX_HW_COUNTERS],
}

#[repr(C)]
struct Luid {
    low: u32,
    high: i32,
}

#[repr(C)]
struct TokenPrivileges {
    count: u32,
    luid: Luid,
    attributes: u32,
}

#[link(name = "advapi32")]
unsafe extern "system" {
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn LookupPrivilegeValueW(system: *const u16, name: *const u16, luid: *mut Luid) -> i32;
    fn AdjustTokenPrivileges(
        token: Handle,
        disable_all: i32,
        new_state: *const TokenPrivileges,
        buffer_len: u32,
        previous: *mut c_void,
        return_len: *mut u32,
    ) -> i32;
    fn TraceQueryInformation(
        session: u64,
        class: u32,
        buf: *mut c_void,
        len: u32,
        ret_len: *mut u32,
    ) -> u32;
    fn TraceSetInformation(session: u64, class: u32, buf: *const c_void, len: u32) -> u32;
    fn StartTraceW(session: *mut u64, name: *const u16, props: *mut EventTraceProperties) -> u32;
    fn ControlTraceW(
        session: u64,
        name: *const u16,
        props: *mut EventTraceProperties,
        code: u32,
    ) -> u32;
}

unsafe extern "system" {
    fn GetCurrentProcess() -> Handle;
    fn GetCurrentThread() -> Handle;
    fn GetLastError() -> u32;
    fn CloseHandle(handle: Handle) -> i32;
    fn EnableThreadProfiling(thread: Handle, flags: u32, hw_counters: u64, out: *mut Handle)
    -> u32;
    fn DisableThreadProfiling(profiling: Handle) -> u32;
    fn ReadThreadProfilingData(profiling: Handle, flags: u32, out: *mut PerformanceData) -> u32;
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn unavailable(counter: Counter, why: impl std::fmt::Display) -> Unavailable {
    Unavailable {
        reason: format!(
            "{}: {why}. The Windows backend is experimental; see docs/hardware-counters.md",
            layer_name(counter)
        ),
    }
}

/// Explains a Win32 error from one of the PMC calls.
fn explain(code: u32) -> String {
    match code {
        ERROR_NOT_SUPPORTED => "Windows reports no hardware counter support (error 50). A \
             hypervisor, including Hyper-V and virtualization-based security, commonly hides the \
             CPU's counters"
            .to_owned(),
        ERROR_ACCESS_DENIED => {
            "access denied; run from an elevated (administrator) terminal".to_owned()
        }
        ERROR_ALREADY_EXISTS => "another tool already owns the NT Kernel Logger session".to_owned(),
        other => format!("Win32 error {other}"),
    }
}

/// Parses the buffer `TraceQueryInformation(TraceProfileSourceListInfo)`
/// fills: a chain of `PROFILE_SOURCE_INFO` records, each with
/// `NextEntryOffset`, `Source`, two intervals, a reserved `u64`, then a
/// NUL-terminated UTF-16 description. Malformed input yields what parsed cleanly
/// and then stops; it never reads out of bounds.
pub(super) fn parse_profile_sources(buf: &[u8]) -> Vec<(u32, String)> {
    const HEADER: usize = 24;
    let mut out = Vec::new();
    let mut off = 0_usize;
    while off + HEADER <= buf.len() {
        let u32_at =
            |at: usize| u32::from_le_bytes([buf[at], buf[at + 1], buf[at + 2], buf[at + 3]]);
        let next = u32_at(off) as usize;
        let source = u32_at(off + 4);
        let end = if next == 0 {
            buf.len()
        } else {
            (off + next).min(buf.len())
        };
        if end < off + HEADER {
            break;
        }
        let name: Vec<u16> = buf[off + HEADER..end]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .take_while(|&c| c != 0)
            .collect();
        out.push((source, String::from_utf16_lossy(&name)));
        if next == 0 {
            break;
        }
        off += next;
    }
    out
}

/// Turns on `SeSystemProfilePrivilege`, which ETW PMC access requires. An
/// administrator holds it but it is off by default.
fn enable_profile_privilege() -> Result<(), u32> {
    let mut token: Handle = std::ptr::null_mut();
    // SAFETY: the pseudo handle from `GetCurrentProcess` needs no cleanup, and
    // `token` is a live out-pointer.
    let ok = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    };
    if ok == 0 {
        // SAFETY: reads the calling thread's last error.
        return Err(unsafe { GetLastError() });
    }
    let mut privileges = TokenPrivileges {
        count: 1,
        luid: Luid { low: 0, high: 0 },
        attributes: SE_PRIVILEGE_ENABLED,
    };
    let name = wide("SeSystemProfilePrivilege");
    // SAFETY: `name` is NUL-terminated UTF-16 and `privileges.luid` is a live
    // out-pointer. `token` is a real handle, closed on every path below.
    let result = unsafe {
        let adjusted = LookupPrivilegeValueW(std::ptr::null(), name.as_ptr(), &mut privileges.luid)
            != 0
            && AdjustTokenPrivileges(
                token,
                0,
                &privileges,
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            ) != 0;
        if adjusted {
            // `AdjustTokenPrivileges` succeeds even when the privilege is not
            // held; the last error says so.
            match GetLastError() {
                ERROR_NOT_ALL_ASSIGNED => Err(ERROR_ACCESS_DENIED),
                _ => Ok(()),
            }
        } else {
            Err(GetLastError())
        }
    };
    // SAFETY: `token` came from `OpenProcessToken` and is closed once.
    unsafe { CloseHandle(token) };
    result
}

/// The PMC sources this machine offers, as `(id, name)`.
fn query_sources() -> Result<Vec<(u32, String)>, u32> {
    let mut buf = vec![0_u8; 1 << 16];
    let mut returned = 0_u32;
    // SAFETY: `buf` is writable for `buf.len()` bytes and `returned` is a live
    // out-pointer. Session handle 0 is valid for this query class.
    let rc = unsafe {
        TraceQueryInformation(
            0,
            TRACE_PROFILE_SOURCE_LIST_INFO,
            buf.as_mut_ptr().cast(),
            buf.len() as u32,
            &mut returned,
        )
    };
    if rc != 0 {
        return Err(rc);
    }
    buf.truncate((returned as usize).min(buf.len()));
    Ok(parse_profile_sources(&buf))
}

const LOGGER_NAME: &str = "NT Kernel Logger";

/// The kernel trace session, stopped when dropped.
struct Session {
    handle: u64,
    /// Properties plus the session name; kept for `ControlTraceW`.
    props: Vec<u64>,
}

impl Session {
    fn start(source: u32) -> Result<Self, u32> {
        let name = wide(LOGGER_NAME);
        let header = size_of::<EventTraceProperties>();
        let total = header + name.len() * 2;
        // Backed by `u64`s so the structure at the front is 8-byte aligned.
        let mut props = vec![0_u64; total.div_ceil(8)];
        let p = props.as_mut_ptr().cast::<EventTraceProperties>();
        // SAFETY: `props` is zeroed, large enough for the structure plus the name,
        // and aligned for `EventTraceProperties` (8 bytes). The name is copied into
        // the bytes after the header, inside the allocation.
        unsafe {
            (*p).wnode_buffer_size = total as u32;
            (*p).wnode_flags = WNODE_FLAG_TRACED_GUID;
            (*p).guid = SYSTEM_TRACE_CONTROL_GUID;
            (*p).log_file_mode = EVENT_TRACE_REAL_TIME_MODE;
            (*p).logger_name_offset = header as u32;
            std::ptr::copy_nonoverlapping(
                name.as_ptr().cast::<u8>(),
                p.cast::<u8>().add(header),
                name.len() * 2,
            );
        }
        let mut handle = 0_u64;
        // SAFETY: `handle` is a live out-pointer, `name` is NUL-terminated and
        // `p` is the initialised properties block above.
        let rc = unsafe { StartTraceW(&mut handle, name.as_ptr(), p) };
        if rc != 0 {
            return Err(rc);
        }
        let session = Self { handle, props };
        let ids = [source];
        // SAFETY: `ids` is a live array of one `u32`, and the length passed is its
        // size in bytes.
        let rc = unsafe {
            TraceSetInformation(
                handle,
                TRACE_PMC_COUNTER_LIST_INFO,
                ids.as_ptr().cast(),
                size_of_val(&ids) as u32,
            )
        };
        if rc != 0 {
            return Err(rc); // `session` drops here and stops the trace.
        }
        Ok(session)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let name = wide(LOGGER_NAME);
        let p = self.props.as_mut_ptr().cast::<EventTraceProperties>();
        // SAFETY: `props` is the block used to start this session, still alive
        // and aligned. `handle` is a session started by `StartTraceW`.
        unsafe { ControlTraceW(self.handle, name.as_ptr(), p, EVENT_TRACE_CONTROL_STOP) };
    }
}

/// A PMC counter for the calling thread.
#[derive(Debug)]
pub(crate) struct Pmc {
    profiling: Handle,
    start: u64,
    // Declared last so thread profiling is disabled before the session stops.
    _session: SessionGuard,
}

/// `Session` without a `Debug` bound on its buffer.
struct SessionGuard(#[allow(dead_code)] Session);

impl std::fmt::Debug for SessionGuard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Session")
    }
}

impl Pmc {
    pub(crate) fn open(counter: Counter) -> Result<Self, Unavailable> {
        // Step 2 first: asking which sources exist is the cheapest way to learn
        // that the machine has none, and it is the step that fails under a
        // hypervisor. It needs no privilege to ask.
        let sources = query_sources().map_err(|code| unavailable(counter, explain(code)))?;
        let wanted = windows_source_names(counter);
        let Some(name) = pick_first_available(wanted, |candidate| {
            sources
                .iter()
                .any(|(_, n)| n.eq_ignore_ascii_case(candidate))
        }) else {
            return Err(unavailable(
                counter,
                "this CPU offers no PMC source for that event",
            ));
        };
        let source = sources
            .iter()
            .find(|(_, n)| n.eq_ignore_ascii_case(name))
            .map(|(id, _)| *id)
            .ok_or_else(|| unavailable(counter, "the PMC source list changed during setup"))?;

        enable_profile_privilege().map_err(|code| unavailable(counter, explain(code)))?;
        let session = Session::start(source).map_err(|code| unavailable(counter, explain(code)))?;

        let mut profiling: Handle = std::ptr::null_mut();
        // SAFETY: the pseudo handle for the current thread is valid for this
        // call, bit 0 selects the first configured counter, and `profiling` is
        // a live out-pointer.
        let rc = unsafe {
            EnableThreadProfiling(
                GetCurrentThread(),
                THREAD_PROFILING_FLAG_DISPATCH,
                1,
                &mut profiling,
            )
        };
        if rc != 0 || profiling.is_null() {
            return Err(unavailable(counter, explain(rc)));
        }
        let mut pmc = Self {
            profiling,
            start: 0,
            _session: SessionGuard(session),
        };
        // Prove the counter reads back before reporting it as available.
        if pmc.read_now().is_none() {
            return Err(unavailable(
                counter,
                "Windows returned no hardware counter data after setup",
            ));
        }
        pmc.start = 0;
        Ok(pmc)
    }

    fn read_now(&self) -> Option<u64> {
        // SAFETY: an all-zero `PerformanceData` is a valid value (plain integers).
        let mut data: PerformanceData = unsafe { std::mem::zeroed() };
        data.size = size_of::<PerformanceData>() as u16;
        data.version = 1;
        // SAFETY: `profiling` is a live handle from `EnableThreadProfiling` and
        // `data` is a writable `PerformanceData` of the size it declares.
        let rc = unsafe {
            ReadThreadProfilingData(
                self.profiling,
                READ_THREAD_PROFILING_FLAG_HARDWARE_COUNTERS,
                &mut data,
            )
        };
        (rc == 0 && data.hw_counters_count >= 1).then(|| data.hw[0].value)
    }

    pub(crate) fn start(&mut self) {
        self.start = self.read_now().unwrap_or(0);
    }

    pub(crate) fn end(&mut self) -> f64 {
        self.read_now()
            .map_or(0.0, |now| now.wrapping_sub(self.start) as f64)
    }
}

impl Drop for Pmc {
    fn drop(&mut self) {
        // SAFETY: `profiling` came from `EnableThreadProfiling` and is disabled
        // once. The session field drops after this body runs.
        unsafe { DisableThreadProfiling(self.profiling) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(next: u32, source: u32, name: &str) -> Vec<u8> {
        let mut r = Vec::new();
        r.extend_from_slice(&next.to_le_bytes());
        r.extend_from_slice(&source.to_le_bytes());
        r.extend_from_slice(&[0; 16]); // intervals and the reserved u64
        for unit in name.encode_utf16().chain(std::iter::once(0)) {
            r.extend_from_slice(&unit.to_le_bytes());
        }
        r
    }

    #[test]
    fn parses_a_chain_of_profile_sources() {
        let first = record(0, 1, "TotalIssues");
        let mut buf = record(first.len() as u32, 1, "TotalIssues");
        buf.extend(record(0, 7, "InstructionRetired"));
        let parsed = parse_profile_sources(&buf);
        assert_eq!(
            parsed,
            vec![
                (1, "TotalIssues".to_owned()),
                (7, "InstructionRetired".to_owned())
            ]
        );
    }

    #[test]
    fn malformed_buffers_never_read_out_of_bounds() {
        assert!(parse_profile_sources(&[]).is_empty());
        assert!(parse_profile_sources(&[0; 10]).is_empty());
        // A next-offset pointing far past the end is clamped, not followed.
        let mut buf = record(10_000, 3, "X");
        buf.truncate(30);
        let _ = parse_profile_sources(&buf);
    }

    #[test]
    fn explains_the_errors_the_probe_actually_saw() {
        assert!(explain(50).contains("hypervisor"));
        assert!(explain(5).contains("elevated"));
        assert!(explain(183).contains("NT Kernel Logger"));
    }

    #[test]
    fn event_trace_properties_matches_the_documented_size() {
        assert_eq!(size_of::<EventTraceProperties>(), 120);
        assert_eq!(size_of::<PerformanceData>(), 32 + 16 * 16);
    }
}
