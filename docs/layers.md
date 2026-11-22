# Measurement layers

A layer is one probe that reports one metric for each batch of iterations.
Each benchmark runs once per active layer, so a routine measured under
`WallClock` and `Allocations` is executed in two separate passes. The passes do
not affect each other. See [`architecture.md`](architecture.md).

| Layer (macro name) | Value per iteration | Platforms | Setup needed |
|--------------------|---------------------|-----------|--------------|
| `WallClock` | Nanoseconds | Linux, macOS, Windows, other (see below) | None |
| `Allocations` | Heap allocation calls | Any, with the counting allocator | `#[global_allocator]` |
| `AllocBytes` | Heap bytes requested | Any, with the counting allocator | `#[global_allocator]` |
| `HwInstructions`, `HwL1Misses`, `HwL2Misses`, `HwBranchMisses` (or `HardwareCounters` for all four) | Events per iteration | Linux only in this build | Kernel permission (see below) |

When a layer cannot run, the runner prints a warning naming the layer and
skips it. The other layers still run. `Allocations::new()` and
`AllocBytes::new()` do not check for the allocator, so use `try_new()` if you
need the check.

## WallClock

Elapsed time for each batch, divided by the iteration count. The clock
(`crates/tpt-benchmark/src/clock.rs`) is chosen in this order:

| Platform | Source | Used when |
|----------|--------|-----------|
| x86 / x86_64 | `RDTSC` | The CPU reports an invariant TSC (CPUID `0x8000_0007`, EDX bit 8), and the calibrated rate is between 1 and 10 GHz. The rate is not architecturally exposed, so it is calibrated against `Instant` over 10 ms. |
| aarch64 | `CNTVCT_EL0` | The calibrated rate agrees with `CNTFRQ_EL0` to within 5 %. |
| Any | `std::time::Instant` | Any other case, including when a hardware source fails its check. |

`Clock::best()` makes this choice. `Clock::instant()` forces the portable
source. `WallClock::new()` uses `Clock::best()`.

The aarch64 source compiles for `aarch64-unknown-linux-gnu`, but it has not
yet been run on aarch64 hardware.

## Allocations and AllocBytes

Both layers read thread-local counters that a `CountingAllocator` updates.
The allocator forwards every call to `System` and adds to the counters on
success:

| Operation | `Allocations` (calls) | `AllocBytes` (bytes) |
|-----------|-----------------------|----------------------|
| `alloc` | +1 | layout size |
| `alloc_zeroed` | +1 | layout size |
| `realloc` | +1 | new size |
| `dealloc` | not counted here | not counted here |

Install the allocator in the binary that runs the benchmark:

```rust
#[global_allocator]
static ALLOC: tpt_benchmark::alloc::CountingAllocator =
    tpt_benchmark::alloc::CountingAllocator::new();
```

Only one global allocator is allowed per binary, and the library cannot
install one for you. See [`allocations.md`](allocations.md) for the details,
the multi-threading rule (only the thread that runs the batch is counted), and
what "requested bytes" means.

`Allocations::try_new()` and `AllocBytes::try_new()` make one probe allocation
and return `Unavailable`, with the install instructions, when it is not
counted.

**Stop reason for zero-allocation routines.** A routine that makes no
allocations gives a value of 0 on every batch. The engine never meets its
precision target for a zero mean, so the run ends at `MaxSamples` (1000 draws)
or `MaxTime`, not `Converged`. See [`statistics.md`](statistics.md#behaviour-the-caps-imply).
The result is still exactly 0.

## HardwareCounters

Four events are available, each read as a count for each batch:

| Layer | Event | Linux mapping |
|-------|-------|---------------|
| `HwInstructions` | Retired instructions | `PERF_COUNT_HW_INSTRUCTIONS` |
| `HwL1Misses` | L1 data cache read misses | `PERF_TYPE_HW_CACHE`, L1D, read, miss |
| `HwL2Misses` | Last-level cache read misses | `PERF_TYPE_HW_CACHE`, LL, read, miss |
| `HwBranchMisses` | Mispredicted branches | `PERF_COUNT_HW_BRANCH_MISSES` |

`HwL2Misses` maps to the last-level cache, because the generic perf events
have no L2 entry. On most current CPUs that is L3. The counters count user
space only (`exclude_kernel`, `exclude_hv`).

**Platform support in this build:**

- **Linux:** implemented with `perf_event_open`. It needs
  `kernel.perf_event_paranoid` at 2 or lower, or `CAP_PERFMON`. Containers,
  VMs and CI runners often hide the PMU.
- **macOS:** not implemented. kperf is a private API that needs root.
- **Windows:** not implemented. PMC access needs ETW or a driver, and admin.
- **Other:** not supported.

On macOS, Windows and other platforms the layer reports `Unavailable`, and
the runner skips it with a warning.

Setup, container and VM notes, and the event mapping are in
[`hardware-counters.md`](hardware-counters.md).

## Choosing layers in code

```rust
#[tpt_bench]
#[layers(WallClock, Allocations)]
fn parse(b: &mut Bencher) { /* ... */ }

#[tpt_bench_group]
#[layers(HardwareCounters)]
mod kernels { /* ... */ }
```

`#[layers]` can sit on a group or on one benchmark. A benchmark's own
attribute replaces the group's. With no attribute, a benchmark runs under
`WallClock` only.
