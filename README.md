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
| `HardwareCounters` | Instructions, cache and branch misses     | Linux (`perf_event_open`). **Experimental** on macOS (`sudo`) and Windows (administrator): written without hardware to verify them. Elsewhere, or when unavailable, the layer is skipped with a reason |

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

## Examples

Runnable examples are in
[`crates/tpt-benchmark/examples/`](crates/tpt-benchmark/examples/). From a
clone of this repository:

| Example | Shows | Run |
|---------|-------|-----|
| `quickstart` | one parameterised benchmark with throughput | `cargo run --release --example quickstart -- --quick` |
| `allocations` | counting allocations, a function that allocates nothing | `cargo run --release --example allocations -- --quick` |
| `batched_and_async` | per-iteration setup outside the timing, async routines | `cargo run --release --example batched_and_async -- --quick` |
| `measure_directly` | the engine as a library: `measure()` and the `Summary` | `cargo run --release --example measure_directly` |

## Publish results to GitHub Pages

Any repository can publish a benchmark trend site from a short workflow. The
action runs your benches, adds the results to a history kept on the `gh-pages`
branch, and rebuilds the site. Nothing is installed from crates.io.

```yaml
name: Benchmarks
on:
  push: { branches: [main] }
  schedule: [{ cron: "0 5 * * 1" }]
permissions:
  contents: write
jobs:
  bench:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: tpt-solutions/tpt-benchmark@master
        with: { bench: sums }   # your bench target name(s)
```

Then set Pages to deploy from the `gh-pages` branch in the repository settings.

Inputs: `bench` (space-separated bench targets; empty runs a plain
`cargo bench`), `bench-args` (for example `--quick`), `title`, `threshold`
(fail the job when a benchmark is slower than the previous run on the same
machine by more than this many percent), `publish` (`false` for a dry run) and
`branch`. For several operating systems, use a matrix with `max-parallel: 1`
so the runs push one at a time. Runs from different machines are charted
separately.

Hosted runners are noisy and usually have no hardware counters, so read the
trend on one machine rather than the absolute numbers.

To build the site locally:

```sh
cargo install --git https://github.com/tpt-solutions/tpt-benchmark tpt-site
cargo bench
tpt-site build --input target/tpt-benchmark   # writes site/, and renders ./docs if present
tpt-site serve                                # http://127.0.0.1:8000/
```

## Migrating from Criterion

The `#[tpt_bench]` API and CLI flags follow Criterion's conventions where that
is practical. The differences, a table mapping Criterion's API to this one, and
short notes for Divan and Iai users are in
[`docs/migrating-from-criterion.md`](docs/migrating-from-criterion.md).

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
