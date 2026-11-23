# Unsafe code policy

`tpt-benchmark` keeps its `unsafe` surface as small as possible.

## Rules

1. `tpt-benchmark` has `#![deny(unsafe_code)]` at the crate root. A module may
   opt in with `#[allow(unsafe_code)]`, and only the modules listed below do.
   `tpt-benchmark-macros` and `tpt-site` have no `unsafe` and use
   `#![forbid(unsafe_code)]`.
2. Every `unsafe` block carries a `// SAFETY:` comment that states the
   invariant it relies on and why it holds.
3. Every `pub unsafe fn` documents its preconditions in a `# Safety` section.
   The crate has none. The `unsafe fn` methods of the `GlobalAlloc` impl take
   their contract from that trait.
4. `unsafe_op_in_unsafe_fn` is denied workspace-wide, so an unsafe operation
   inside an `unsafe fn` still needs its own block and comment.
5. New `unsafe` needs a test or a documented reason it cannot be tested.
6. Prefer a safe standard-library API when the MSRV allows it. (The no-op waker
   used to be hand-written `unsafe`; it is now `Waker::noop()`, stable from 1.85.)

## Inventory

Audited on 2026-11-23 against the rules above. The audit was a manual review of
every site, plus the checks listed under "How each is verified".

| Site | Unsafe operation | Why it is sound |
|------|------------------|-----------------|
| `clock.rs` `raw_ticks` (x86, x86_64) | `_rdtsc()` | No memory effects. Selected only after the invariant-TSC check. Faults if user-space RDTSC is disabled (see limitations). |
| `clock.rs` `invariant_tsc_supported` (x86, x86_64) | `__cpuid` (unsafe on the 1.85 MSRV, safe on newer toolchains, hence `allow(unused_unsafe)`) | CPUID exists on every x86_64 CPU. On 32-bit x86 it is assumed present, since the CPUs Rust's x86 targets can realistically run on have it. The extended leaf is read only after confirming the CPU reports it. |
| `clock.rs` `raw_ticks`, `cntfrq` (aarch64) | `mrs` of `CNTVCT_EL0` and `CNTFRQ_EL0` | Side-effect free and readable at EL0 on Linux and macOS. Windows on ARM has not been checked. The `CNTVCT_EL0` read does not use `nomem`, so the compiler cannot move memory operations of the measured code across it. |
| `alloc/counting.rs` `impl GlobalAlloc` | `System.alloc`, `alloc_zeroed`, `dealloc`, `realloc` | Each call forwards its arguments unchanged, so `System` sees exactly the contract the caller promised. The counters are `const`-initialised, destructor-free thread-locals read with `try_with`, so counting never allocates and cannot re-enter the allocator. |
| `hwcounters/linux.rs` `Perf::open` | `libc::syscall(SYS_perf_event_open, ..)` | `PerfEventAttr` is `repr(C)`, fully initialised, and its `size` field is `size_of` it. A unit test pins that size at 112 bytes (`PERF_ATTR_SIZE_VER5`). The kernel reads it only during the call. |
| `hwcounters/linux.rs` `Perf::read_now` | `libc::read` | Reads exactly 24 bytes into a 24-byte buffer. A short read is treated as failure. The fd is owned and open. |
| `hwcounters/linux.rs` `Perf::drop` | `libc::close` | The fd came from a successful open and `drop` runs once. |
| `hwcounters/macos.rs` (experimental) | `dlopen`, `dlsym` and calls through the resolved function pointers into the private `kperf` / `kperfdata` frameworks | Strings are NUL-terminated literals, null results are checked, and every out-pointer and buffer is a live local whose size is passed correctly. **What cannot be checked:** the function signatures are from memory, so a changed signature in a macOS release is undetected. Never run against real counters. |
| `hwcounters/windows.rs` (experimental) | `OpenProcessToken`, `LookupPrivilegeValueW`, `AdjustTokenPrivileges`, `TraceQueryInformation`, `StartTraceW`, `TraceSetInformation`, `ControlTraceW`, `EnableThreadProfiling`, `ReadThreadProfilingData` | The properties block is zeroed, aligned and sized for the structure plus the session name, with the layout size pinned by a test. Handles are closed or stopped on every path. Only the first query has run, and it fails with "not supported" on the development machine. The rest follows the documented sequence and has never run. |
| `report/win_console.rs` | `GetStdHandle`, `GetConsoleMode`, `SetConsoleMode` | Documented kernel32 signatures. The handle is checked for null and `INVALID_HANDLE_VALUE`, and `SetConsoleMode` runs only on a handle `GetConsoleMode` accepted. |

### Findings from the audit, and what changed

- **Removed one `unsafe` block.** The hand-written `RawWaker` in
  `Bencher::iter_async` is now `Waker::noop()`.
- **Corrected the `RDTSC` justification.** It said "no preconditions". It can
  fault when user-space `RDTSC` is disabled, which is now stated.
- **Tightened the `CNTVCT_EL0` read** by dropping `nomem` (see the table).
- **Made hardware counters thread-bound in the type system.** The counter
  follows the thread that opened it, so reading it from another thread would
  silently measure the wrong thread. `HardwareCounter` is now neither `Send` nor
  `Sync`, on every platform, with a `compile_fail` doctest. Making that possible
  meant removing the `Send + Sync` supertrait from `MeasurementLayer`, which the
  crate never needed.
- **Pinned the kernel ABI size** of `PerfEventAttr` with a unit test.
- **Added `forbid(unsafe_code)`** to the macros crate and `tpt-site`.

## Known limitations

- **`RDTSC` can fault.** Where user-space `RDTSC` is disabled, for example
  after `prctl(PR_SET_TSC, PR_TSC_SIGSEGV)` or under some sandboxes and
  record-and-replay tools, the first read raises `SIGSEGV`. The crate does not
  detect this. There is no switch to force the `Instant` clock yet. Constructing
  `Clock::instant()` directly avoids it.
- **Counter reads are not serialised.** Neither `RDTSC` nor `mrs CNTVCT_EL0` is
  ordered against earlier instructions by the CPU, so a read can complete
  slightly before the work it follows. Batches last at least 1 ms by default,
  so the effect is far below the measurement noise. It would matter for a
  single very short timing, which this crate does not do.
- **A leaked fd on an unreachable path.** If `perf_event_open` returned a value
  that does not fit a `c_int`, the descriptor would not be closed. The kernel
  never returns one, so this is not handled.
- **Perf counters have been exercised on Linux only as far as opening.** See
  below.

## How each is verified

| Check | Covers | Does not cover |
|-------|--------|----------------|
| `cargo clippy -D warnings`, `cargo test` on Linux, macOS and Windows (CI) | All safe and unsafe code that runs on those targets, including the `GlobalAlloc` wrapper and the console calls | Anything that needs hardware the runner lacks |
| Cross-target `cargo check` (aarch64 Linux, aarch64 Windows) | That the `CNTVCT_EL0` and `CNTFRQ_EL0` code compiles | Running it. The Apple Silicon `macos-latest` job is the first real run |
| Linux run under WSL2 (Ubuntu, Rust 1.85) | The Linux backend compiles and its tests pass, including the ABI-size test; `perf_event_open` is attempted and its `ENOENT` is turned into the documented "unavailable" message | A successful counter read: WSL2 hides the PMU, so that path needs a real machine |
| `cargo clippy` and `cargo check` for `aarch64-apple-darwin` and `x86_64-apple-darwin`, including on 1.85 | That the macOS backend compiles and passes the lints with correct types | Running it. macOS CI will exercise the not-root path; real counts are unverified |
| Windows tests on the development machine | The Windows backend's real first step (PMC source query, which fails cleanly), the source-list parser against synthetic and malformed buffers, and the structure sizes | Everything after the query, which never succeeds here |
| Miri job on Linux CI (`continue-on-error` until its first green run) | `GlobalAlloc` wrapper and thread-local counters, the engine, batching, baselines, the no-op waker, under strict provenance | Inline asm and syscalls. Under Miri the clock uses `Instant` and the perf counter reports itself unavailable, so the `RDTSC`, `CPUID`, `mrs` and perf blocks are covered only by review and the checks above |

Still open (see `todo.md`): hardware tests on real machines for each OS, and a
sanitizer run.
