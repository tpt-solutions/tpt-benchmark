# tpt-benchmark

A layered, statistically sequential benchmarking framework for Rust.

`tpt-benchmark` measures several things in one run: wall-clock time, heap
allocations, and hardware counters. It stops sampling as soon as the 95 %
confidence interval is tight enough, rather than after a fixed number of
runs.

> **Status: pre-1.0.** The core engine, the wall-clock layer, the macros and
> the reports are in progress. See [`todo.md`](todo.md) for the current state.

## Layers

| Layer              | Measures                                  | Platforms                         |
|--------------------|-------------------------------------------|-----------------------------------|
| `WallClock`        | Nanoseconds per iteration                 | All (TSC / `CNTVCT_EL0` / `Instant`) |
| `Allocations`      | Heap allocation calls per iteration       | All, with the counting allocator installed |
| `HardwareCounters` | Instructions, cache and branch misses     | Linux (`perf_event_open`); others degrade gracefully |

## Minimal example

```rust,ignore
use tpt_benchmark::{tpt_bench, tpt_bench_group, layers, Bencher, black_box};

#[tpt_bench_group]
#[layers(WallClock)]
mod sums {
    use super::*;

    #[tpt_bench(args = [128, 1024])]
    fn sum(b: &mut Bencher, n: usize) {
        let data: Vec<u64> = (0..n as u64).collect();
        b.iter(|| black_box(&data).iter().sum::<u64>());
    }
}

tpt_benchmark::main!(sums);
```

Declare the target in `Cargo.toml` with `harness = false`:

```toml
[[bench]]
name = "sums"
harness = false
```

Run it with `cargo bench`. Useful flags:

```text
--list                  list benchmarks
--quick                 shorter, less precise run
--save-baseline <name>  save results under target/tpt-benchmark/baselines/
--baseline <name>       compare against a saved baseline
--threshold <pct>       regression threshold (default 5)
--noplot                skip the HTML report
```

Output is written to `target/tpt-benchmark/`: a JSON export
(`tpt-benchmark.json`), baselines, and an HTML report.

## Migrating from Criterion

The `#[tpt_bench]` API and CLI flags follow Criterion's conventions where that
is practical. Differences are listed in the migration guide, which is still to
be written.

## Minimum supported Rust version

1.85, on the stable channel. The crate uses Rust edition 2024.

## Licence

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this work by you, as defined in the Apache-2.0 licence, shall be
dual licensed as above, without any additional terms or conditions.

Dependencies are restricted to permissive licences: MIT, dual MIT/Apache-2.0,
BSD, ISC, Zlib and Unicode. Apache-2.0-only, GPL, LGPL, AGPL and other
copyleft licences are rejected. `cargo deny` enforces this in CI (see
[`deny.toml`](deny.toml)).
