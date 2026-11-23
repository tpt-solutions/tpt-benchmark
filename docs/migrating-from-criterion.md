# Migrating from Criterion

This guide maps a Criterion benchmark suite onto `tpt-benchmark`. It covers
the code changes, the command-line flags, the output locations and what has
no equivalent yet. Features that are not implemented are listed at the end,
so check that section before you start.

The Criterion-side names below follow Criterion's documented API as we know
it. They were not re-checked against a Criterion release for this guide. The
`tpt-benchmark` side was checked against the source.

## Cargo.toml

Keep `harness = false`. Remove the `criterion` dev-dependency and add
`tpt-benchmark`:

```toml
[dev-dependencies]
tpt-benchmark = "0.1"        # not yet on crates.io; use a path or git dependency until it is published

[[bench]]
name = "sums"
harness = false
```

## A single benchmark

Criterion:

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn sum(c: &mut Criterion) {
    c.bench_function("sum", |b| b.iter(|| black_box(1 + 1)));
}

criterion_group!(benches, sum);
criterion_main!(benches);
```

`tpt-benchmark`:

```rust
use tpt_benchmark::{Bencher, black_box, tpt_bench, tpt_bench_group};

#[tpt_bench_group]
mod arith {
    use super::*;

    #[tpt_bench]
    fn sum(b: &mut Bencher) {
        b.iter(|| black_box(1 + 1));
    }
}

tpt_benchmark::main!(arith);
```

Points to note:

- `criterion_group!` and `criterion_main!` are replaced by `tpt_bench_group`
  on a module and `tpt_benchmark::main!`. Each `main!` argument is a group
  module.
- The benchmark function takes `b: &mut Bencher` and must call one of the
  measuring methods (`b.iter`, `b.iter_batched`, `b.iter_async`) exactly once.
  It must return `()`.
- The benchmark's name is `group::function`, so `sum` in `mod arith` is
  `arith::sum`. Filters and baselines use that name.
- `black_box` is re-exported from `std::hint::black_box`. Import it from
  `tpt_benchmark`, not from `criterion`.

## Benchmark groups and functions

| Criterion | tpt-benchmark |
|-----------|---------------|
| `criterion_group!(name, f1, f2)` | `#[tpt_bench_group] mod name { ... }` with `#[tpt_bench]` functions |
| `c.benchmark_group("g")` | A module named `g` with `#[tpt_bench_group]` |
| `c.bench_function("f", \|b\| ...)` | A function `f` with `#[tpt_bench]` |
| `criterion_main!(name)` | `tpt_benchmark::main!(name)` |

## Parameterised benchmarks

Criterion's `BenchmarkId` and `bench_with_input` become `args = [...]`. Each
value produces one benchmark. Its name is the function name, a slash, and the
text of the expression:

```rust
#[tpt_bench(args = [128, 1024])]
fn sum(b: &mut Bencher, n: usize) {
    let data: Vec<u64> = (0..n as u64).collect();
    b.iter(|| black_box(&data).iter().sum::<u64>());
}
```

This is benchmarked as `arith::sum/128` and `arith::sum/1024`. Build the
input once, outside `iter`, as shown.

## Layers

Criterion measures wall time only. In `tpt-benchmark`, wall time is the
default layer and needs no attribute. The other layers are selected with
`#[layers(...)]` on a group or on one benchmark:

```rust
#[tpt_bench_group]
#[layers(WallClock, Allocations)]
mod parse { /* ... */ }
```

Valid names are `WallClock`, `Allocations`, `AllocBytes`, `HardwareCounters`,
`HwInstructions`, `HwL1Misses`, `HwL2Misses` and `HwBranchMisses`. The
allocation layers need `#[global_allocator]` (see
[`allocations.md`](allocations.md)). Hardware counters work on Linux only (see
[`layers.md`](layers.md)).

## Command-line flags

Run the bench as before with `cargo bench`. Arguments after `--` go to the
binary.

| Criterion flag | tpt-benchmark | Notes |
|----------------|---------------|-------|
| `<FILTER>` | `<FILTER>` | Substring match only. Criterion takes a regex. |
| `--exact` | `--exact` | Same meaning. |
| `--list` | `--list` | Prints `group::name` for each benchmark. |
| `--save-baseline <NAME>` | `--save-baseline <NAME>` | Writes to `target/tpt-benchmark/baselines/<NAME>.json`. |
| `--baseline <NAME>` | `--baseline <NAME>` | Compares against that file. Exits with code 2 if it is missing or has an unsupported format. |
| `--noplot` | `--noplot` | Skips the HTML report. |
| `--quick` | `--quick` | Different settings: warm-up 20 ms, `rel_error` 5 %, sampling cap 1 s. |
| `--threshold <PCT>` | `--threshold <PCT>` | Regression threshold in percent. Default 5. |
| `--confidence 0.95` | `--confidence 0.95` | Accepted only with the value 0.95. Any other value is an error. |
| `--bench` | Ignored | Cargo passes it. |

tpt-benchmark exits with code 1 when a significant regression exceeds the
threshold, which makes it usable as a CI gate.

Options not listed above are an error (exit code 2), for example
`--sample-size`, `--measurement-time`, `--warm-up-time`, `--load-baseline` and
`--significance-level`. Remove them from scripts.

## Output locations

| | Criterion | tpt-benchmark |
|--|-----------|---------------|
| Directory | `target/criterion/` | `target/tpt-benchmark/` (honours `CARGO_TARGET_DIR`) |
| Machine-readable data | `target/criterion/<bench>/new/estimates.json` (one per benchmark) | `target/tpt-benchmark/tpt-benchmark.json` (one file for the run) |
| Baselines | One directory per benchmark | `target/tpt-benchmark/baselines/<NAME>.json` (one file per baseline) |
| HTML report | `target/criterion/report/index.html` | `target/tpt-benchmark/index.html` |

The JSON file is versioned (`schema_version`) and is described in
[`json-schema.md`](json-schema.md). Its contents differ from Criterion's
`estimates.json`, so update any scripts that read it.

Baseline names may contain letters, digits, `-`, `_` and `.`. Any other
character is rejected. Baselines saved by Criterion cannot be read by
tpt-benchmark, and the reverse is also true. Re-save the baselines you need
with the new tool.

## Configuration

Criterion lets you set sample size, measurement time and warm-up time per
group. tpt-benchmark does not have per-benchmark configuration. The `Config`
(warm-up, minimum batch, engine settings) is used for every benchmark, and the
`--quick` flag is the only tuning switch on the command line. Set your own
`Config` by calling `tpt_benchmark::measure` directly if you need other values.

## What is not supported

These are checked against `crates/tpt-benchmark/src/bencher.rs` and the macros
crate. They have not been implemented.

- **`iter_batched_ref`, `iter_with_setup` and the `BatchSize` argument.** Use
  `Bencher::iter_batched(setup, routine)`, which exists. Its setup runs
  outside the timed region, and each batch is capped at 1024 iterations
  because the inputs for a batch are built together. The Criterion names
  and the `BatchSize` enum have no counterpart.
- **Async `async fn` benchmark functions.** The `#[tpt_bench]` macro rejects
  them with "async benchmark functions are not supported". Write a normal
  function and call `Bencher::iter_async(|| async { ... })`, or
  `Bencher::iter_async_with(executor, ...)` to use your runtime's `block_on`.
- **Confidence levels other than 0.95.** The engine computes a 95 % interval
  only.
- **Regex filters.** The filter is a substring match, or an exact match with
  `--exact`.
- **Separate report pages.** The HTML report is one `index.html` with a
  section per benchmark and inline SVG charts. There are no per-group or
  per-benchmark pages.
- **Per-benchmark configuration** through attributes (see above).
- **Criterion-compatible files.** No shim. `criterion::` imports cannot be
  replaced by a re-export.

If your suite uses one of these, plan to keep it on Criterion until it is
supported.

## Notes for Divan and Iai users

The tool-side names below come from those projects' public documentation as we
know it. They were not re-checked against a current release, so confirm them
upstream. The `tpt-benchmark` side was checked against the source.

### Coming from Divan

Divan and `tpt-benchmark` both use attribute macros and a `harness = false`
bench target, so the shape of a suite carries over.

| Divan | tpt-benchmark |
|-------|---------------|
| `#[divan::bench]` on a function | `#[tpt_bench]`. The function must take `b: &mut Bencher` first. A function without that parameter is a compile error. |
| `#[divan::bench(args = [...])]` | `#[tpt_bench(args = [...])]`. Each value becomes `name/<arg>`. |
| `#[divan::bench_group]` on a module | `#[tpt_bench_group]` on a module. |
| `fn main() { divan::main(); }` | `tpt_benchmark::main!(group, ...)`, naming each group module. |
| `divan::black_box` | `tpt_benchmark::black_box`. |
| `Bencher::with_inputs(..).bench_values(..)` | `b.iter_batched(setup, routine)`. Setup runs outside the timing. |
| A routine that returns a value, benchmarked directly | `b.iter(\|\| routine())`. The return value goes through `black_box`. |
| Throughput counters (bytes, items) | `b.throughput(Throughput::Bytes(n))` or `Throughput::Elements(n)`. The rate is in the summary and the JSON export, not yet in the terminal table. |
| `AllocProfiler` as the global allocator | `CountingAllocator` as the global allocator, with the `Allocations` and `AllocBytes` layers. |

Differences worth knowing:

- **Generic benchmarks.** Divan can run one function over a list of types. There is
  no equivalent here, so write one `#[tpt_bench]` function per type, or a macro of
  your own.
- **How a run ends.** Divan takes a fixed number of samples or a time budget.
  `tpt-benchmark` stops when the 95 % interval is tight enough, with sample and
  time caps. See [`statistics.md`](statistics.md).
- **Threads.** Divan can run a benchmark on several threads at once. The
  allocation layers here count only the thread that runs the batch.
- **No Divan comparison has been run.** `comparison-criterion.md` records why Divan
  stayed out of the comparison, so there are no figures here for how the two
  agree.

### Coming from Iai

Iai (the original `iai` crate, and `iai-callgrind`) counts instructions and
cache accesses by running the benchmark once under Valgrind. That gives a
number that barely varies from run to run, which makes it a good fit for noisy
CI machines. It does not measure time, and it needs Valgrind, so it does not run
on Windows.

`tpt-benchmark` is different in kind, not a replacement:

- **Instructions.** The `HwInstructions` layer reads the CPU's instruction
  counter through `perf_event_open`, so it only works on Linux with permission,
  and not on most hosted CI runners (see
  [`hardware-counters.md`](hardware-counters.md)). The value is a statistical
  per-iteration mean with a confidence interval, and it includes any noise from
  the real machine.
- **No simulation.** There is no cache simulation and no Valgrind dependency.
  Cache and branch misses are hardware counts, not modelled ones.
- **Keep both if you need both.** If you want a deterministic instruction count in
  CI, keep Iai for that. Use `tpt-benchmark` for wall-clock time, allocation counts
  and, on a machine with counter access, instructions.

There is no mechanical conversion. An Iai benchmark is a plain function, so wrap
its body in `b.iter(|| ...)` inside a `#[tpt_bench]` function.
