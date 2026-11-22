# tpt-benchmark vs Criterion: identical workloads

This note records a head-to-head run of tpt-benchmark and Criterion 0.5.1 on
the same four workloads, on the same machine, back to back. It answers two
questions from `todo.md`:

1. Do the two 95% confidence intervals of the mean agree for each workload?
2. How long does each framework take to finish each workload?

## Method

- **Workloads** live in `crates/tpt-benchmark/benches/common/workloads.rs` and
  are included by both benches with `#[path]`, so the measured code is the
  same source file.
- **Criterion side:** `crates/tpt-benchmark/benches/vs_criterion.rs`,
  Criterion 0.5.1 with `default-features = false` (no plots, no rayon).
  Interval read from `target/criterion/<name>/new/estimates.json`,
  `mean.confidence_interval`.
- **tpt side:** `crates/tpt-benchmark/benches/vs_tpt.rs`, `WallClock` layer only,
  default `Config`. Interval read from `target/tpt-benchmark/tpt-benchmark.json`,
  `ci_low` / `ci_high`, plus `stop_reason`.
- **Driver:** `scripts/compare.ps1`. It builds both benches first with
  `--no-run` (compile time excluded), then runs `vs_criterion` and `vs_tpt`
  back to back for 3 rounds. Each `cargo bench` invocation is wall-timed with
  `Measure-Command`. Medians of the CI bounds and the spread across rounds are
  computed from the output files.
- **Reference timer (sanity check, not a framework):** the same `workloads.rs`
  compiled standalone with `rustc -O` and timed with a plain `std::time::Instant`
  loop (best of five). It is only used to judge which framework's numbers look
  right; it is not a statistical method.
- **Environment:** Windows 11, Intel i7-12700 (8 P-cores + 4 E-cores, hybrid),
  release/bench profile. The machine was shared with other agents' cargo and
  miri processes during the run, so the numbers carry some load noise.
  Compile times on Windows are much slower than Linux; they are excluded here.
- **Source revision:** the timed runs used a snapshot of commit `d6280c1`
  (`git archive HEAD`) with this work's `Cargo.toml` and bench files overlaid.
  The live working tree had another agent's in-progress edits to
  `src/bencher.rs` that did not compile at the time, so the numbers were not
  taken from it. The snapshot's library is therefore the committed one.

### Workloads

| Name | Routine | Notes |
|---|---|---|
| `sum_u64_1024` | sum of a `Vec<u64>` of 1024 elements, input through `black_box` | vectorisable integer sum |
| `dot_f64_128x128` | dot product of two flattened 128x128 f64 matrices (16384 elements each) | scalar f64 dependency chain |
| `format_string_100` | 100 iterations of `format!("{}-{}", i, i * 2)` appended to a `String` | allocates on every iteration |
| `fast_mul_u64` | `black_box(x).wrapping_mul(3)` with a fixed `x` | about 0.3 to 0.7 ns, stresses timer overhead |

"128x128 f64 dot product" was interpreted as a dot product over the flattened
matrices, because no matrix-vector product was specified.

## Results

### Main run: 3 rounds, medians

CI bounds are the median across the three rounds of each side's lower and
upper bound (ns per iteration). Wall time is the median of the three
`cargo bench` process wall times.

| Workload | Criterion 95% CI (ns) | tpt 95% CI (ns) | Overlap | Criterion wall (s) | tpt wall (s) | tpt stop_reason per round |
|---|---|---|---|---|---|---|
| `sum_u64_1024` | 175.73 to 180.43 | 67.54 to 68.64 | no | 47.2 | 1.8 | converged, converged, converged |
| `dot_f64_128x128` | 14647.67 to 14874.05 | 7169.41 to 7195.25 | no | 47.2 | 1.8 | converged, converged, max_samples |
| `format_string_100` | 13324.12 to 13833.65 | 5592.80 to 5642.10 | no | 47.2 | 1.8 | converged, converged, max_samples |
| `fast_mul_u64` | 0.68 to 0.69 | 0.31 to 0.32 | no | 47.2 | 1.8 | max_samples, max_samples, converged |

Per-round wall times (seconds): Criterion 56.5, 47.2, 44.1; tpt 1.8, 1.8, 4.5.
Round-to-round spread of the mean: Criterion 167.7 to 203.1 ns for the sum
(its per-sample spread alone was 103 to 202 ns); tpt 66.8 to 70.7 ns for the
sum. The two frameworks were each internally stable in the sense that their
intervals did not move between rounds, but they sit at different levels.

### Diagnostic run: 2 interleaved rounds with the reference timer

Run directly from the built binaries, reference first, then tpt, then
Criterion, to check whether the gap survived a different time window.

| Workload | Reference `Instant` loop (ns) | tpt mean (ns) | Criterion mean (ns) |
|---|---|---|---|
| `sum_u64_1024` | 67.9 / 68.2 | 68.6 / 68.6 | 166.2 / 167.6 |
| `dot_f64_128x128` | 7051 / 6852 | 6947 / 7236 | 14428 / 14926 |
| `format_string_100` | 5552 / 5415 | 5719 / 5562 | 13803 / 12912 |
| `fast_mul_u64` | 0.42 / 0.41 | 0.33 / 0.32 | 0.68 / 0.71 |

An earlier standalone reference run gave 83, 7496, 6637 and 0.47 ns; it was
taken under more load, so the later pair is the more representative one.

## Conclusions

1. **Confidence intervals do not agree on any of the four workloads.** The
   intervals on each side are narrow (about 1 to 3 percent wide for Criterion
   and about 1 percent for tpt on the larger workloads), but they are at
   different levels. Criterion is about 2.0 to 2.6 times higher than tpt on every
   workload, in every round, including the diagnostic round.

2. **The disagreement is not explained by load or by the tpt side being wrong.**
   The reference timer, which uses neither framework, agrees with tpt to within
   a few percent on the sum, dot product and format workloads, and to within
   about 0.1 ns on the fast multiply. Criterion's own raw samples for the sum
   are spread across 103 to 202 ns with a median of 168 ns, so Criterion's
   measured loop is slower than the reference on this machine. I did not
   identify the cause inside Criterion's harness (candidate factors: the
   hybrid P/E core layout during its longer run, and its per-sample overhead
   on Windows). That is an open item, not a finding.

3. **Wall time: tpt reaches its result faster on this suite.** Median process
   wall time is about 1.8 s per tpt bench run against about 47 s for Criterion,
   roughly 25 times less. This mostly reflects the default time budgets: the
   Criterion defaults used here (5 s measurement and 3 s warm-up per benchmark)
   versus tpt's adaptive stopping, which converged or hit `max_samples` in
   about a second per workload. It is not a like-for-like comparison of
   time-to-equal-precision, because the two frameworks do not target the same
   interval (see point 1).

4. **Is "faster than Criterion" supported?** Only partly, and only on the wall
   time measure. On this machine tpt finished the suite about 25 times faster
   in wall time. Its per-iteration numbers also match the independent reference
   more closely than Criterion's do, so the intervals disagree with Criterion
   rather than validating it. The claim "same 95% CI as Criterion" is **not**
   supported by this run. The accuracy claim should be stated against a
   reference, not against Criterion.

5. **Stop reasons matter.** Several tpt runs ended at `max_samples` rather
   than `converged` (dot product and format in round 3; fast multiply in
   rounds 1 and 2). Those intervals are not at the precision target the
   convergence rule aims for, and the wall-time figure for them is a cap, not
   a convergence time.

## Divan

Divan is **deferred**, not added. Adding it would need another dev-dependency,
another bench target and another full run for a three-way comparison, and the
two-way result above already fails the agreement test. A Divan comparison is
worth doing only after the Criterion interval disagreement is explained. This
document is the only place that records the deferral; `todo.md` was not edited.

## Reproducing

```powershell
pwsh -File scripts/compare.ps1 -TargetDir <absolute path to a private target dir> -Runs 3
```

Output: a table on stdout and `<TargetDir>/compare-results.json` with every
round's CI, mean, stop reason and wall time. The Criterion JSON lands in
`<TargetDir>/criterion/`, not `target/criterion/`, because `CARGO_TARGET_DIR` is
set for these runs.

## Limitations

- Single machine, Windows, hybrid CPU, shared with other agents' builds. No
  pinning or isolation was applied.
- Three rounds of the driver plus two diagnostic rounds. The spread across
  rounds is small relative to the Criterion/tpt gap, but this is not a
  statistical test of the gap.
- The reference timer is a single-loop `Instant` measurement, used only as a
  sanity check.
- Criterion was run at 0.5.1. Criterion 0.7.0 is available but was not tested.
- Linux was not measured.
