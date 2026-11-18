# tpt-benchmark — Project TODO

Owner: TPT Solutions · License: `MIT OR Apache-2.0` · Edition 2024 · MSRV 1.85 (stable only)

**Decisions locked in**
- Project is dual-licensed MIT OR Apache-2.0 (supersedes the spec's "MIT-only").
- Dependencies: Apache-2.0-only, GPL, LGPL, AGPL and any other copyleft are banned. Dual MIT/Apache crates are fine.
- Workspace: `tpt-benchmark` (library) + `tpt-benchmark-macros` (proc macros).
- HW counters on Linux, macOS and Windows in v1.0.
- Output dir: `target/tpt-benchmark/` (not `target/criterion/`). Criterion migration guide only, no compat shim.
- CI: GitHub Actions (Linux/macOS/Windows). Publish to crates.io.
- Extra scope beyond the spec: async benches, throughput units, `iter_batched`-style setup/teardown, CI regression exit codes.
- HTML reports: plotters SVG embedded in a single HTML file + small vanilla JS.

---

## Phase 0 — Foundations (Week 0)
- [ ] Initialise git repo, `.gitignore`, `.gitattributes`
- [ ] Create Cargo workspace (`tpt-benchmark`, `tpt-benchmark-macros`)
- [ ] Set `edition = "2024"`, `rust-version = "1.85"`, `license = "MIT OR Apache-2.0"`, authors/repository metadata
- [ ] Add `LICENSE-MIT` (copyright TPT Solutions)
- [ ] Add `LICENSE-APACHE` (Apache-2.0 text, copyright TPT Solutions)
- [ ] README license section with standard dual-license + contribution wording
- [ ] Write `deny.toml`: allow MIT, BSD-2-Clause, BSD-3-Clause, ISC, Unicode-DFS/Unicode-3.0, Zlib; deny copyleft; reject Apache-2.0-only crates
- [ ] Verify `cargo deny` treats `MIT OR Apache-2.0` crates as OK, and Apache-2.0-only crates fail (test with a throwaway dep)
- [ ] Update spec.txt wording (MIT-only → dual MIT/Apache; output dir; plotters election note)
- [ ] `rustfmt.toml`, clippy config, `#![forbid(unsafe_code)]` where possible, document every `unsafe` block policy
- [ ] GitHub Actions: fmt, clippy, test, doc on Linux/macOS/Windows
- [ ] GitHub Actions: `cargo deny check` job
- [ ] GitHub Actions: MSRV 1.85 job
- [ ] CONTRIBUTING.md, CODE_OF_CONDUCT.md, SECURITY.md, issue/PR templates
- [ ] Choose dependency shortlist and check each licence (plotters, serde/serde_json, proc-macro2/syn/quote, libc/windows-sys, a colour crate, etc.)
- [ ] Decide on a benchmark baseline: Criterion and Divan benchmarks of identical workloads, for the "faster than Criterion" claim

## Phase 1 — Core Engine (Weeks 1–3)
- [ ] Define `Bencher<'a>` and `b.iter(...)`
- [ ] Define `MeasurementLayer` trait (`start_batch`, `end_batch`, `Sample: SampleData`) and `SampleData` trait
- [ ] Define `StatisticalEngine` trait (`should_stop`); decide on a generic vs. typed sample (spec's `&[Sample]` is ambiguous)
- [ ] `black_box` re-export (`std::hint::black_box`)
- [ ] Batched-iteration loop: run N iterations, measure once, divide by N
- [ ] Batch-size calibration/auto-scaling (start ~10 iterations, grow geometrically)
- [ ] Measure and document timing overhead (target <1 ns per iteration amortised)
- [ ] Warm-up phase
- [ ] WallClock layer: `_rdtsc` on x86/x86_64
- [ ] WallClock layer: `cntvct_el0` on aarch64
- [ ] WallClock layer: `std::time::Instant` fallback
- [ ] TSC checks: invariant-TSC detection, frequency calibration, fallback when unreliable
- [ ] Sequential engine: running mean/variance (Welford)
- [ ] Sequential engine: confidence-interval computation (95%)
- [ ] Sequential engine: SPRT implementation
- [ ] Sequential engine: Bayesian stopping rule
- [ ] Sequential engine: adaptive sampling (stop early when variance low and CI tight)
- [ ] Sequential engine: hard caps (max time, max samples) and min samples
- [ ] MAD-based outlier detection and real-time rejection
- [ ] Multi-layer orchestration: re-run the loop per layer when layers can't share a run
- [ ] Unit tests with known synthetic distributions; property tests for the stats
- [ ] Validation: same 95% CI as Criterion on identical workloads
- [ ] Validation: false-stop rate / coverage test of the SPRT
- [ ] Benchmark vs. Criterion (wall time to reach result)
- [ ] **Milestone:** basic time benchmarks run and are faster than Criterion

## Phase 2 — Specialist Layers (Weeks 4–6, extended for 3-OS HW support)
### Allocations layer
- [ ] Counting `GlobalAlloc` wrapper (alloc, dealloc, realloc, alloc_zeroed counts + bytes)
- [ ] Decide how a user installs the allocator (macro-generated `#[global_allocator]` vs. explicit opt-in) and document the limitation
- [ ] Scope counting to the benchmark region only (thread-local/atomic counters, ignore harness allocations)
- [ ] Multi-threaded benchmark correctness
- [ ] Tests proving a known zero-alloc function reports exactly 0 allocs / 0 bytes
- [ ] Tests for known-allocation functions (Vec, String, Box)
### HardwareCounters layer
- [ ] Common abstraction: instructions retired, L1 misses, L2 misses, branch mispredictions
- [ ] Linux: `perf_event_open` (grouped counters, multiplexing scaling, `perf_event_paranoid` handling)
- [ ] macOS: `kperf` / kpc (private framework, requires root; handle Apple Silicon and Intel)
- [ ] Windows: PMC via supported API (investigate ETW/PMC counters, admin rights, VM limitations)
- [ ] Graceful degradation: clear warning + skip layer when counters unavailable (containers, VMs, CI, no permission)
- [ ] Event-name mapping per CPU vendor/arch (Intel, AMD, Apple, ARM)
- [ ] Hardware tests on real machines for each OS (not VM-only)
- [ ] Docs: permissions/setup per OS
- [ ] **Milestone:** can prove zero-allocation and instruction-count reductions

## Phase 3 — "One-Stop Shop" UX (Weeks 7–9, extended for extra scope)
### Macros
- [ ] `tpt-benchmark-macros` crate skeleton (syn/quote/proc-macro2 — licence-checked)
- [ ] `#[tpt_bench]` attribute (name, `args = [...]` parameterisation)
- [ ] `#[tpt_bench_group]` on modules
- [ ] `#[layers(WallClock, Allocations, HardwareCounters)]` attribute parsing
- [ ] `tpt_benchmark::main!(group, ...)` entry point (harness = false)
- [ ] Helpful compile errors with spans; trybuild tests
- [ ] Re-export macros from `tpt-benchmark`
### API surface
- [ ] `iter_batched`-style setup/teardown API (per-iteration and per-batch setup)
- [ ] Throughput units (bytes/sec, elements/sec) set per benchmark
- [ ] Async benchmarks (executor-agnostic, `b.iter(|| async {..})`), with a tested example for at least one executor
- [ ] Benchmark filtering via CLI args (name substring/regex), `--list`, `--exact`
- [ ] CLI flags: `--save-baseline`, `--baseline`, `--noplot`, `--quick`, `--confidence`, etc.
- [ ] Compatibility with `cargo bench` and `cargo test --benches` (smoke-run mode)
### Baselines & regression
- [ ] Baseline save/load in `target/tpt-benchmark/`
- [ ] Baseline file format versioning
- [ ] Regression detection (statistical comparison, noise threshold, significance level)
- [ ] Configurable regression threshold + non-zero exit code on regression for CI
- [ ] Compare against baselines from another commit (named baselines, import/export)
### JSON export
- [ ] JSON schema design and documentation (versioned)
- [ ] Write `target/tpt-benchmark/tpt-benchmark.json` (raw statistical data per layer)
- [ ] JSON schema tests / golden files
- [ ] **Milestone:** Criterion-parity API, cleaner and more powerful

## Phase 4 — Native Reporting (Weeks 10–12)
- [ ] Terminal tables: columns per active layer (time, allocs, bytes, instructions, cache/branch misses)
- [ ] Colourised deltas vs. baseline (-14.5% green, +5.2% red), respect `NO_COLOR` and non-TTY
- [ ] Windows terminal colour support
- [ ] plotters integration (SVG backend only, no gnuplot, no heavy system libs); confirm plotters + its transitive deps pass `cargo deny` under MIT election
- [ ] Charts: throughput, distribution histogram, regression slope, baseline comparison
- [ ] Self-contained HTML report (single file, no CDN/external assets) with inline SVG
- [ ] Small vanilla JS for toggles/tooltips (no JS dependencies)
- [ ] Per-group index page + per-benchmark pages
- [ ] Report light/dark theme
- [ ] Report tests (snapshot SVG/HTML)
- [ ] Final `cargo deny` CI gate enforced on all targets
- [ ] **Milestone:** reports complete

## Phase 5 — Hardening, Docs, Migration (Weeks 13–14)
- [ ] Audit all `unsafe` code (TSC, allocator, FFI for perf/kperf/PMC); document invariants
- [ ] Miri / sanitizer runs where applicable
- [ ] Cross-platform test matrix incl. aarch64 (Linux + Apple Silicon + Windows-on-ARM if feasible)
- [ ] Stress tests: noisy machine, very fast (<1 ns) and very slow (>1 s) benchmarks
- [ ] Performance validation: confirm "15 s → 2–3 s" claim on a defined suite, with identical 95% CI
- [ ] Dogfood: write benchmarks for `tpt-math`, `tpt-cv`, `tpt-gis` (zero-alloc and SIMD instruction proofs)
- [ ] rustdoc for all public items; `#![deny(missing_docs)]`
- [ ] README, quickstart, examples directory
- [ ] mdBook or docs pages: architecture, statistics method, layers, permissions per OS
- [ ] Criterion → tpt-benchmark migration guide (and Divan/Iai notes)
- [ ] CHANGELOG.md; semver and stability policy
- [ ] Review the public API for 1.0 stability (trait signatures, `#[non_exhaustive]`)

## Phase 6 — Release v1.0.0
- [ ] Verify crates.io name availability for `tpt-benchmark` and `tpt-benchmark-macros`
- [ ] Complete Cargo.toml metadata (description, keywords, categories, readme, docs)
- [ ] `cargo publish --dry-run` for both crates; publish macros first, then library
- [ ] Release workflow (tag → GitHub release → publish)
- [ ] docs.rs build verified (feature flags, platform cfgs)
- [ ] Final `cargo deny` + licence audit of the full dependency tree
- [ ] Tag `v1.0.0`, announce
- [ ] **Milestone:** v1.0.0 released

## Post-1.0 backlog
- [ ] `cargo tpt-benchmark` CLI subcommand (compare/manage baselines)
- [ ] Additional counters / user-selectable events
- [ ] Criterion-compat shim (if demand)

---

## Open risks / things to watch
- [ ] macOS `kperf` is a private API and needs root; may break between macOS versions
- [ ] Windows PMC access needs admin/ETW and is often unavailable in VMs/CI
- [ ] HW counters on GitHub-hosted runners are generally unavailable → needs a self-hosted or manual test machine
- [ ] TSC reliability (non-invariant, VM migration, frequency scaling)
- [ ] Spec's "same 95% CI in 2–3 s" claim needs empirical validation; SPRT/Bayesian rules can be anti-conservative if mis-tuned
- [ ] plotters transitive dependency licences (verify every one passes `deny.toml`)
- [ ] `unsafe` footprint must stay small and audited
