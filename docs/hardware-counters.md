# Hardware counters: permissions and setup

The `HardwareCounter` layer reads CPU events: instructions retired, L1 data
cache misses, L2 cache misses and branch mispredictions. Access depends on the
OS, the privileges of the user, and whether the machine exposes its PMU at all.
When a counter cannot be opened, the layer reports `Unavailable` with a reason.
The runner prints a warning and skips that layer. Nothing else fails.

| OS      | Status in this build |
|---------|----------------------|
| Linux   | Implemented with `perf_event_open`. Opens and degrades correctly; counts need real hardware to check. |
| macOS   | **Experimental.** Private `kperf` frameworks, needs root. Written without a Mac; never run against real counters. |
| Windows | **Experimental.** ETW PMC sources plus thread profiling, needs administrator. Reports unavailable on the one machine it was tried on; the counting path has never run. |
| Other   | Not supported. |

**What "experimental" means here.** The macOS and Windows backends are part of
the 1.0 API, but they were written without being able to test them against
working hardware counters. Treat their numbers as unverified until a run on
real hardware says otherwise (the todo tracks that). They are built so that any
problem, such as a missing framework, a denied call or a changed system, makes
the layer report `Unavailable` with a reason and skip, never panic or print
nonsense. The exception is a *changed signature* in a private macOS function,
which cannot be detected (see below). Only the Linux backend has been checked
as far as opening a counter.

## Linux

### Permission model

`perf_event_open` is governed by `/proc/sys/kernel/perf_event_paranoid`:

| Value | Meaning for an unprivileged user                                       |
|-------|-------------------------------------------------------------------------|
| -1    | No restriction (kernel and user counting, raw tracepoints).            |
| 0     | Kernel profiling and CPU events allowed.                               |
| 1     | Per-process events allowed; kernel profiling restricted.               |
| 2     | Per-process events allowed for user space only (`exclude_kernel`).     |
| 3     | Denied for unprivileged users on distros with the patch (e.g. Debian). |

The layer always counts user space only (`exclude_kernel` and `exclude_hv` are
set). It therefore works at the default level **2**, which most distros ship.
At **3** it needs `CAP_PERFMON` or root.

Check and change the level:

```sh
cat /proc/sys/kernel/perf_event_paranoid
sudo sysctl -w kernel.perf_event_paranoid=2      # until reboot
echo 'kernel.perf_event_paranoid = 2' | sudo tee /etc/sysctl.d/99-perf.conf   # persistent
```

Alternatively, grant the capability to the benchmark binary without root:

```sh
sudo setcap cap_perfmon=ep target/release/deps/<bench-binary>
```

Re-run `setcap` after every rebuild, because the binary is replaced.
`CAP_PERFMON` (Linux 5.8 and later) is preferred over `CAP_SYS_ADMIN`.

### Containers

- Docker and Podman block `perf_event_open` by default through seccomp. Use
  `--cap-add=PERFMON` (Linux 5.8 and later), or
  `--security-opt seccomp=unconfined` if the capability is not enough.
- The host's `perf_event_paranoid` still applies inside the container.
- Kubernetes: add `SYS_PERFMON` to the container's capabilities.

### Virtual machines

Hypervisors often hide the PMU from guests. The layer then reports that the
event is not exposed (`ENOENT`, `EOPNOTSUPP` or `EINVAL` from the kernel).
Typical cases:

- Cloud VMs without PMU passthrough (most general-purpose instances).
- Some CI runners, including GitHub-hosted runners.
- Nested virtualisation without vPMU enabled.

Use a bare-metal or self-hosted machine for hardware counter benchmarks.

### Event mapping

The layer uses the generic perf events, which Intel, AMD and ARM all expose:

| Counter      | perf event                                             |
|--------------|--------------------------------------------------------|
| Instructions | `PERF_TYPE_HARDWARE` / `PERF_COUNT_HW_INSTRUCTIONS`    |
| BranchMisses | `PERF_TYPE_HARDWARE` / `PERF_COUNT_HW_BRANCH_MISSES`   |
| L1Misses     | `PERF_TYPE_HW_CACHE` / L1D, read, miss                 |
| L2Misses     | `PERF_TYPE_HW_CACHE` / LL (last-level), read, miss     |

L2 maps to the **last-level cache**, because the generic events have no L2
entry. On most current CPUs the LL cache is L3, so `HwL2Misses` counts L3
misses there. Read the number as "last-level cache misses".

Vendor-specific raw events (for example Intel `MEM_LOAD_RETIRED.L2_MISS`) are
out of scope for v1.

### Reading and multiplexing

The counter is opened for the thread that creates the layer and stays enabled.
Each batch reads the value and subtracts the reading taken at the start. Each
read also returns the time the event was enabled and the time it was actually
on a PMU. If the kernel multiplexes events, the delta is scaled by
`enabled / running`. A counter that never ran in the batch reports 0.

## macOS

**Experimental.** `kperf` and `kperfdata` are private Apple frameworks with no
header and no stability promise. The backend (`hwcounters/macos.rs`) finds them
at run time with `dlopen` and each function with `dlsym`, then:

1. takes the counters (`kpc_force_all_ctrs_set`), which **needs root**, so run
   the benchmark with `sudo`;
2. looks the event up by name in this CPU's event database (`kpep_*`), trying
   Apple Silicon names first and Intel names after;
3. programs and starts the counters, and reads the calling thread's counts with
   `kpc_get_thread_counters`, subtracting a reading taken at the start of each batch.

Dropping the layer switches counting off and gives the counters back.

What is and is not known:

- It type-checks and lints for `aarch64-apple-darwin` and `x86_64-apple-darwin`.
  It has **not been run**, so the call sequence, the function signatures and the
  event names are from memory of how other tools use these frameworks.
- A missing framework, a missing symbol, a missing event or a refused call all
  give `Unavailable`. A function whose **signature changed** in a macOS release
  is not detectable and could misbehave.
- The counters are system-wide. While the layer is alive it takes them from
  other users of the PMU, such as Instruments.
- Virtual machines, including GitHub's macOS runners, are expected to hide the
  counters, so CI can check the unavailable path but probably not real counts.
- Which events exist differs by CPU. If none of the candidate names is in the
  database, that counter is unavailable.

## Windows

**Experimental.** The backend (`hwcounters/windows.rs`) uses only documented
user-mode APIs, with no driver:

1. enable `SeSystemProfilePrivilege` on the process token (**needs an
   administrator**, so run from an elevated terminal);
2. ask ETW which PMC "profile sources" the CPU offers
   (`TraceQueryInformation`, `TraceProfileSourceListInfo`) and pick the one for
   the event by name;
3. start the NT Kernel Logger session and give it that source as its PMC counter
   list (`TraceSetInformation`, `TracePmcCounterListInfo`);
4. enable thread profiling for the calling thread and read the counter with
   `ReadThreadProfilingData`, subtracting a reading taken at the start of each batch.

Starting the kernel logger is system-wide: it fails if another tool owns it, and
it is stopped when the layer is dropped.

What is and is not known:

- **Step 2 has been run, and fails on the development machine.** There, Windows
  answers with `ERROR_NOT_SUPPORTED` with or without elevation (see below), so the
  layer reports `Unavailable` and says a hypervisor commonly causes that. That
  path is covered by tests.
- **Steps 3 and 4 have never run.** They follow Microsoft's documented
  sequence from memory, including struct layouts and the counter-index
  convention, and have not been checked against working counters.
- The source names (`InstructionRetired`, `BranchMispredictsRetired`,
  `LLCMisses`, ...) are from memory of `xperf -pmcsources` on Intel CPUs. There is
  no generic L1 source, so `HwL1Misses` is always unavailable on Windows.
- Hosted CI runners and other virtual machines are expected to hide the
  counters.

Windows benchmarks can always use the `Allocations`, `AllocBytes` and `WallClock`
layers. See `docs/allocations.md`.

### Investigation (2026-11-23)

A throwaway probe called the user-mode APIs Windows documents for this, on a
Windows 11 Pro machine (build 26200) with an Intel Core i7-12700 and bare-metal
hardware, but with Hyper-V present and virtualization-based security running
(it hosts WSL2 and Docker). It was run twice, once as a normal user and once
from an elevated (administrator) terminal. The results were identical:

| Call | Result |
|------|--------|
| `TraceQueryInformation(TraceProfileSourceListInfo)`, the list of PMC sources ETW can record | Win32 error 50, `ERROR_NOT_SUPPORTED`, with and without elevation |
| `EnableThreadProfiling` with hardware counters requested | Succeeds, but `ReadThreadProfilingData` reports `HwCountersCount = 0` |
| `QueryThreadCycleTime` | Works, and returns thread CPU cycles |

What this means:

- Administrator rights are not what blocks it: the elevated run failed the same
  way, with "not supported" rather than "access denied".
- With no PMC sources listed, there is nothing to configure in an ETW session,
  so the ETW route (the one Windows documents for counters) was not tried further.
- `QueryThreadCycleTime` counts cycles, not retired instructions, so it cannot
  back the `HwInstructions` layer.
- The likely cause is the hypervisor, which commonly hides the CPU's counters
  from the OS above it. That was **not confirmed**: the probe was not run with the
  hypervisor off, which would also disable WSL2 and Docker on that machine.

So the counting path of the Windows backend could not be developed or tested on
this machine as it is configured, and hosted CI runners are virtual machines
that are expected to hide the counters as well. The experimental backend
therefore reports itself as unavailable here, with the explanation above.

## Other platforms

The layer reports "hardware counters are not supported on this platform".

## Hardware tests

Tests that read real counters are marked
`#[ignore = "requires Linux perf access"]`. Run them on a Linux machine where
the user has access:

```sh
cargo test -p tpt-benchmark -- --ignored
```
