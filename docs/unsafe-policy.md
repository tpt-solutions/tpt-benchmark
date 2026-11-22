# Unsafe code policy

`tpt-benchmark` keeps its `unsafe` surface as small as possible.

## Rules

1. The crate root has `#![deny(unsafe_code)]`. A module may opt in with
   `#[allow(unsafe_code)]`, and that attribute is only added to modules that
   need it: the clock, the allocator, and the hardware-counter backends.
2. Every `unsafe` block carries a `// SAFETY:` comment that states the
   invariant it relies on and why it holds.
3. Every `unsafe fn` documents its preconditions in a `# Safety` rustdoc section.
4. `unsafe_op_in_unsafe_fn` is denied workspace-wide, so an unsafe operation
   inside an `unsafe fn` still needs its own block and comment.
5. New `unsafe` needs a test or a documented reason it cannot be tested.

## Inventory

| Module                  | Why it is unsafe                                              |
|-------------------------|----------------------------------------------------------------|
| `clock`                 | `RDTSC` and `CNTVCT_EL0`/`CNTFRQ_EL0` reads, and `CPUID`        |
| `alloc` (planned)       | `GlobalAlloc` implementation delegating to `System`             |
| `hwcounters` (planned)  | `perf_event_open`, `ioctl`, `read` on a file descriptor (Linux) |

## Why each block is sound

- **`RDTSC`/`RDPMC`-style reads** have no memory effects and no preconditions
  on x86. The TSC is used only after CPUID confirms an invariant TSC.
- **`CPUID`** is always available on x86 and x86_64. The extended leaf is
  queried only after the maximum extended leaf has been checked. On toolchains
  where `__cpuid` is safe, the block is still required for MSRV 1.85.
- **`CNTVCT_EL0` / `CNTFRQ_EL0`** are readable at EL0 on the platforms this
  crate targets (Linux and macOS). They have no side effects.

## Status

The Phase 5 audit (todo.md) will re-check each block against these rules,
run Miri where it applies, and record the results here.
