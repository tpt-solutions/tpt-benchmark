# Allocation layers: installing the counting allocator

The `Allocations` (calls per batch) and `AllocBytes` (bytes requested per
batch) layers count heap activity through a wrapper around the system
allocator. The wrapper has to be the process's global allocator, so you
install it yourself:

```rust
#[global_allocator]
static ALLOC: tpt_benchmark::alloc::CountingAllocator =
    tpt_benchmark::alloc::CountingAllocator::new();
```

Put this in the benchmark binary's root (the `main.rs` of a `harness = false`
bench, or a test crate). The library cannot generate it: Rust allows exactly
one global allocator per binary, and a library must not pick it for the user.

## Limitations

- **One allocator per binary.** If your program already has a
  `#[global_allocator]` (for example mimalloc or jemalloc), the counting
  wrapper must replace it. Wrap that allocator yourself, or use the counting
  layers in a separate binary.
- **Installation is not detected by `new()`.** `Allocations::new()` does not
  check. If the allocator is missing, every batch reports 0, which looks like
  a zero-allocation result. Use `Allocations::try_new()` or
  `AllocBytes::try_new()` instead. They make one probe allocation and return
  `Unavailable`, with the install instructions, if it was not counted.
- **Calling thread only.** Counters are thread-local. A batch counts the
  allocations of the thread that runs it and ignores allocations made by other
  threads. For a multi-threaded routine, the measurement covers the thread
  that runs the batch, not the worker threads it spawns or uses.
- **Bytes are requested bytes.** `AllocBytes` sums the sizes passed to
  `alloc` and `alloc_zeroed`, plus the new size passed to `realloc`. It is
  gross allocation volume, not net retained memory. Deallocations are counted
  separately, by `tpt_benchmark::alloc::thread_dealloc_totals`.
- **Overhead.** The counters are plain thread-local `Cell<u64>`s. Each
  allocation adds a few integer operations, with no atomics or locks.

## What is counted

| Operation      | Calls               | Bytes       |
|----------------|---------------------|-------------|
| `alloc`        | +1 on success       | layout size |
| `alloc_zeroed` | +1 on success       | layout size |
| `realloc`      | +1 on success       | new size    |
| `dealloc`      | counted separately  | layout size |

Failed allocations are not counted. Only the batch between `start_batch` and
`end_batch` is measured. The orchestrator's own bookkeeping, such as the `Vec`
pushes after each batch, runs outside that window and is not counted.

## Zero-allocation proofs

```rust
use tpt_benchmark::alloc::Allocations;

let layer = Allocations::try_new()?; // Err(Unavailable) if not installed
```

`crates/tpt-benchmark/tests/alloc_counting.rs` holds the reference checks. A
zero-allocation function reports exactly 0 calls and 0 bytes.
`Vec::with_capacity(16)`, `Box::new(1u64)` and `String::from("hello")` report
1 call each, of 128, 8 and 5 bytes.
