# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project
uses [Semantic Versioning](https://semver.org/) from 1.0.0 onward. Before 1.0,
minor versions may contain breaking changes.

## [Unreleased]

### Added

- `tpt-site` (not published; install with `cargo install --git`): `tpt-site build`
  turns the JSON export into a static site (index, one page per benchmark with
  per-machine history charts, sitemap, robots.txt, OpenGraph and JSON-LD) and
  keeps every run under `data/` so trends accumulate. `tpt-site serve` previews it. `--docs DIR` (default `./docs`) renders Markdown files as pages under `docs/`. The index page has an "About these numbers" section covering runner noise and the unresolved Criterion comparison. `--fail-on-regression` exits non-zero when the newest run is slower
  than the previous run on the same machine by more than `--threshold`.
- Composite GitHub Action (`action.yml`) that runs benches, keeps history on the
  `gh-pages` branch and publishes the site, plus a `pages.yml` workflow for this repo.
- JSON export schema 2: an `environment` block (commit, timestamp, OS, arch, CPU,
  cores, rustc, clock source, runner). The `benchmarks` layout is unchanged, so
  schema 1 readers keep working. See `docs/json-schema.md`.
- `tpt-benchmark` library crate with `measure()`, `Config` and `Bencher::iter`,
  which run a routine in batches and return a `Summary` per layer.
- `SequentialEngine` statistical engine: Welford mean and variance, 95 %
  Student-t confidence interval, MAD outlier rejection (3 scaled MADs, 20-value
  burn-in), SPRT noise gate (p0 0.01, p1 0.10, alpha and beta 0.05), precision
  stopping at a relative error of 2 %, and `max_samples` / `max_time` caps.
  See `docs/statistics.md`.
- `MeasurementLayer` and `SampleData` traits for custom layers.
- `WallClock` layer, with `RDTSC` on x86 and x86_64 (invariant TSC only),
  `CNTVCT_EL0` on aarch64, and `std::time::Instant` as the fallback.
- `Allocations` and `AllocBytes` layers, with a `CountingAllocator` that the
  user installs as the global allocator, and `try_new()` checks.
- `HardwareCounter` layer for instructions, L1 and L2 (last-level) cache
  misses and branch misses. Implemented with `perf_event_open` on Linux. Other
  platforms report the layer as unavailable.
- Attribute macros in `tpt-benchmark-macros`, re-exported by `tpt-benchmark`:
  `#[tpt_bench]` (with `args = [...]`), `#[tpt_bench_group]`, `#[layers(...)]`,
  and `tpt_benchmark::main!`.
- Benchmark runner for `harness = false` binaries: positional substring filter,
  `--exact`, `--list`, `--save-baseline`, `--baseline`, `--threshold`,
  `--quick`, `--noplot`, `--confidence 0.95`, and `--bench` ignored for Cargo.
  Exits with code 1 on a significant regression above the threshold.
- Versioned JSON export, `target/tpt-benchmark/tpt-benchmark.json`
  (`schema_version` 1).
- Named baselines in `target/tpt-benchmark/baselines/<name>.json`
  (`format_version` 1), with regression detection on non-overlapping 95 %
  intervals and a threshold.
- Terminal table with one column per layer and coloured deltas against a
  baseline. Colour is off when `NO_COLOR` is set or output is not a terminal.
- Single-file HTML report, `target/tpt-benchmark/index.html`, with inline
  plotters SVG charts, a light and dark theme, and a small script with no
  external dependencies.
- `black_box` re-exported from `std::hint`.
- `Bencher::iter_batched` (setup outside the timed region, batches capped at
  1024 iterations), `Bencher::iter_async` and `Bencher::iter_async_with`
  (async routines, with a built-in or a caller-supplied executor).
- `Throughput::Bytes` and `Throughput::Elements`, set with
  `Bencher::throughput`. The summary reports a rate per second.
- Documentation: architecture, statistics, layers, hardware counters,
  allocations, JSON schema, unsafe policy, dependencies, timing overhead, and
  a migration guide from Criterion.

### Not yet implemented

- Async `async fn` benchmark functions in the attribute macros (use
  `Bencher::iter_async` inside a normal function).
- A Bayesian stopping rule. The engine uses the precision rule.
- Hardware counters on macOS and Windows.
- Named baseline import and export across commits.

### Fixed

- `Summary` could not read back numbers the JSON export writes as `null`
  (non-finite values such as the std dev of a zero-allocation layer), which broke
  loading such exports and baselines. They now read back as `NaN`.
