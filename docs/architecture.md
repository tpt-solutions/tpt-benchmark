# Architecture

`tpt-benchmark` is split into layers that do not know about each other. A
measurement layer reads one metric. The execution loop runs the routine in
batches and asks the layer for a value. The statistical engine decides which
values to keep and when to stop. The runner, reports and baselines consume the
resulting `Summary` values and never touch the loop itself.

## Diagram

```text
  #[tpt_bench] / #[layers] / tpt_benchmark::main!        (tpt-benchmark-macros)
            |  generate BenchDef tables
            v
  +---------------------------------------------------------------+
  | runner: CLI parsing, filtering, progress output               |
  |   run_benches() --> one measure() per (benchmark, layer)      |
  +-------------------------------+-------------------------------+
                                  |
                                  v
  +---------------------------------------------------------------+
  | bencher: measure(config, layer, |b| b.iter(routine))          |
  |   warm-up -> calibrate batch size -> batched sampling loop    |
  |        |                         |                            |
  |        | start_batch / end_batch | is_outlier / should_stop  |
  |        v                         v                            |
  |   MeasurementLayer           StatisticalEngine (engine.rs)    |
  |   (layer.rs)                 MAD rejection, SPRT, CI, caps    |
  |        |                         |                            |
  |   WallClock / Allocations /   Welford, t-CI, MAD (stats.rs)   |
  |   AllocBytes / HardwareCounter                                 |
  +-------------------------------+-------------------------------+
                                  |  Summary per layer
                                  v
  +---------------------------------------------------------------+
  | BenchResult list                                              |
  |   |-- json.rs        target/tpt-benchmark/tpt-benchmark.json   |
  |   |-- baseline.rs    target/tpt-benchmark/baselines/<name>.json|
  |   |     compare() -> Comparison (regressions, exit code)       |
  |   `-- report/        terminal table, index.html (plotters SVG) |
  +---------------------------------------------------------------+
```

## Modules

| Module | Role |
|--------|------|
| `layer` | `MeasurementLayer` trait (`start_batch`, `end_batch`, `Sample`) and `SampleData`. An erased view lets the loop hold any layer. |
| `clock` | Raw tick sources (`RDTSC`, `CNTVCT_EL0`, `Instant`) with calibrated ns-per-tick. The only module with `unsafe` code, see [`unsafe-policy.md`](unsafe-policy.md). |
| `wallclock` | `WallClock` layer: one `WallSample` (total ns) per batch. |
| `alloc` | `CountingAllocator` (a `GlobalAlloc` wrapper), `Allocations` and `AllocBytes` layers, thread-local counters. |
| `hwcounters` | `HardwareCounter` layer and the `Counter` enum. Linux backend uses `perf_event_open`; other OSes return `Unavailable`. |
| `bencher` | `Config`, `Bencher`, and `measure()`. Owns the batched loop. |
| `engine` | `SequentialEngine` (the default `StatisticalEngine`), `Sprt`, `Observations`. Decides outliers and stop reasons. |
| `stats` | `Welford`, Student-t critical values, `median`, `scaled_mad`, `Summary`, `StopReason`. |
| `runner` | `BenchDef` (the generated table entry), `LayerKind`, `BenchResult` and `main()` are public. `run_benches` and the command-line parser are crate-private. |
| `json` | Versioned JSON export (`schema_version`, currently 2). Only `SCHEMA_VERSION` and `FILE_NAME` are public. |
| `environment` | Crate-private. Captures the machine and build (commit, OS, CPU, rustc, clock source) for the JSON export's `environment` block. |
| `baseline` | Crate-private. Named baselines (`format_version`, currently 1), the statistical `compare()` and `Verdict`. |
| `report` | Crate-internal rendering: the terminal table (colour respects `NO_COLOR` and non-TTY output) and the single-file HTML report. The public part is `#[doc(hidden)]` and exists for `tpt-site`. |
| `macros` | The `tpt_benchmark::main!` macro. The attribute macros live in the `tpt-benchmark-macros` crate and are re-exported. |

## The batched loop

`Bencher::iter` is the basic entry point. `iter_batched`, `iter_async` and
`iter_async_with` build their own batch closure and then call the same
`sample()` function, so the steps below apply to all of them. In order:

1. **Warm-up.** The routine runs as batches of one iteration until
   `Config::warmup` has elapsed. Its values are discarded.
2. **Calibration.** Starting at 10 iterations, the batch size doubles until a
   batch takes at least `Config::min_batch` (default 1 ms), or until
   `Config::max_batch_iters` (default 2^24) is reached. The batch size is then
   fixed for the rest of the run.
3. **Sampling.** Each step runs one batch of that size, takes the layer's
   total for the batch, and divides by the iteration count to get one
   per-iteration value. The engine then:
   - tests the value for being an outlier against the accepted values
     (`is_outlier`), and records the flag for every draw;
   - keeps the value if it is not an outlier;
   - checks `should_stop` and either continues or ends with a `StopReason`.

The result is a `Summary`: mean, standard deviation, median, 95 % interval,
the accepted values, the outlier count, the batch size, the stop reason, and
the `noisy` flag. The `noisy` flag is set whenever the noise test did not
return `Clean`, including the case where it was still undecided.

Because each `measure` call runs the function once, each layer runs the
routine in a separate pass. Layers never share a run, so the cost of one
layer (for example counting allocations) does not change another layer's
timing.

## Extension points

- **New layer:** implement `MeasurementLayer` with a `SampleData` type. It is
  usable with `measure()` immediately. To expose it through the macros, add a
  variant to `LayerKind` and to the layer table in the macros crate.
- **New engine:** implement `StatisticalEngine` (`is_outlier`, `should_stop`,
  `noise_verdict`) and pass it through `Config::engine`. The current `Config`
  holds a concrete `SequentialEngine`, so a different engine needs a change to
  `Config` as well.

## Related documents

- [`statistics.md`](statistics.md): the statistical method, as implemented.
- [`layers.md`](layers.md): what each layer measures and what it needs.
- [`json-schema.md`](json-schema.md): the JSON export.
- [`hardware-counters.md`](hardware-counters.md), [`allocations.md`](allocations.md): setup.
- [`timing-overhead.md`](timing-overhead.md): measured cost of the loop.
- [`unsafe-policy.md`](unsafe-policy.md): where `unsafe` is allowed.
