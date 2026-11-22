# Hardware counters: permissions and setup

The `HardwareCounter` layer reads CPU events: instructions retired, L1 data
cache misses, L2 cache misses and branch mispredictions. Access depends on the
OS, the privileges of the user, and whether the machine exposes its PMU at all.
When a counter cannot be opened, the layer reports `Unavailable` with a reason.
The runner prints a warning and skips that layer. Nothing else fails.

| OS      | Status in this build                                                 |
|---------|----------------------------------------------------------------------|
| Linux   | Implemented with `perf_event_open`. Needs the permissions below.     |
| macOS   | Not implemented. kperf/kpc is a private API that needs root.         |
| Windows | Not implemented. PMC access needs ETW or a kernel driver, and admin. |
| Other   | Not supported.                                                       |

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

kperf and kpc are private frameworks. They need root, and their interface
changes between macOS releases. This build does not link them and reports:

> kperf (private API, root required) not implemented in this build

The layer is therefore unavailable on macOS, including Apple Silicon.

## Windows

Windows exposes PMCs through ETW, kernel-mode drivers and vendor PMU
extensions. Each needs administrator rights, and most are unavailable inside
VMs. This build does not use private APIs and reports:

> hardware counters not supported on Windows in this build; see docs/hardware-counters.md

Windows benchmarks can use the `Allocations`, `AllocBytes` and `WallClock`
layers. See `docs/allocations.md`.

## Other platforms

The layer reports "hardware counters are not supported on this platform".

## Hardware tests

Tests that read real counters are marked
`#[ignore = "requires Linux perf access"]`. Run them on a Linux machine where
the user has access:

```sh
cargo test -p tpt-benchmark -- --ignored
```
