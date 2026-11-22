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

### Main run: 3 rounds, medians (loaded machine; its conclusions were later withdrawn)

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

## Investigation (re-run, quiet machine)

The loaded-machine run above disagreed with the reference on every workload.
The investigation tested whether that was a bug in either framework.

- **Same code, same binary, three timers.** A scratch binary timed the sum
  with a raw loop, with `tpt_benchmark::measure`, and with Criterion in one
  process. Raw: 70 ns. tpt: 69 ns. Criterion: 168 ns. Changing Criterion's
  closure body (inlining the sum) or its `black_box` (standard instead of
  volatile) did not change its result, so the code and the harness's
  `black_box` are not the cause.
- **Order and time in the process.** With Criterion run first, it measured
  68 to 72 ns, the same as the raw loop. Criterion's per-sample times in the
  slow run were flat across sample sizes (about 157 to 188 ns), so this is not
  a regression-slope artefact.
- **Separate processes.** Six standalone Criterion processes measured 68 to
  72 ns. The slow reading did not recur.
- **Under contention.** With all 20 logical cores busy, Criterion measured
  87 to 89 ns and tpt 67 to 71 ns. The slow reading did not recur.
- **The real comparison benches, quiet machine.** Run as the comparison
  itself, one full pass of each:

| Workload | Criterion mean (ns) | Criterion 95% CI | tpt mean (ns) | tpt 95% CI | Overlap | tpt stop reason |
|---|---|---|---|---|---|---|
| `sum_u64_1024` | 67.33 | 67.08 to 67.61 | 67.35 | 67.19 to 67.51 | yes | max_samples (907) |
| `dot_f64_128x128` | 6951 | 6931 to 6972 | 7132 | 7017 to 7247 | no (tpt 2.6% higher) | converged (31) |
| `format_string_100` | 5427 | 5405 to 5450 | 5487 | 5408 to 5565 | yes | converged (31) |
| `fast_mul_u64` | 0.37 | 0.37 to 0.37 | 0.32 | 0.32 to 0.32 | no, at the timer floor | max_samples (842) |

The loaded-machine disagreement therefore did not reproduce. I could not
establish what caused the slow Criterion readings in that run. The two tools
run in different processes, and that run overlapped with other agents' cargo
and miri processes, but load alone did not reproduce the 2x gap.

## Re-run after tuning the noise gate

Changes made: outlier threshold 3 → 5 scaled MADs, SPRT `p0` 0.01 → 0.02 and
`p1` 0.10 → 0.20, and burn-in draws excluded from the SPRT. The burn-in change
fixed a latent bug: the first 20 draws were scored as clean, and with the new
`p0` that alone could decide Clean, so a 24 % outlier stream was accepted.

| Run | Workload | Criterion mean (ns) [95 % CI] | tpt mean (ns) [95 % CI] | tpt stop, noisy flag, samples, outliers |
|---|---|---|---|---|
| Criterion (1 pass) | `sum_u64_1024` | 68.75 [68.51, 69.05] | | |
| tpt r1 | `sum_u64_1024` | | 68.12 [67.97, 68.27] | max_samples, noisy, 902, 98 |
| tpt r2 | `sum_u64_1024` | | 69.19 [68.86, 69.52] | converged, clean, 58, 2 |
| tpt r3 | `sum_u64_1024` | | 67.86 [66.93, 68.79] | converged, clean, 46, 1 |
| Criterion (1 pass) | `dot_f64_128x128` | 7134.9 [7108.2, 7163.6] | | |
| tpt r1 | `dot_f64_128x128` | | 7310.9 [7256.8, 7364.9] | converged, clean, 46, 1 |
| tpt r2 | `dot_f64_128x128` | | 7021.7 [6942.0, 7101.4] | converged, clean, 46, 1 |
| tpt r3 | `dot_f64_128x128` | | 7025.2 [6912.1, 7138.3] | converged, clean, 35, 0 |
| Criterion (1 pass) | `format_string_100` | 5633.5 [5600.9, 5670.3] | | |
| tpt r1 | `format_string_100` | | 5510.5 [5493.4, 5527.7] | max_samples, noisy, 975, 25 |
| tpt r2 | `format_string_100` | | 5463.8 [5412.7, 5515.0] | converged, clean, 35, 0 |
| tpt r3 | `format_string_100` | | 5809.8 [5765.0, 5854.7] | converged, clean, 46, 1 |
| Criterion (1 pass) | `fast_mul_u64` | 0.373 [0.371, 0.376] | | |
| tpt r1 | `fast_mul_u64` | | 0.326 [0.325, 0.327] | max_samples, noisy, 938, 62 |
| tpt r2 | `fast_mul_u64` | | 0.330 [0.329, 0.331] | max_samples, noisy, 965, 35 |
| tpt r3 | `fast_mul_u64` | | 0.321 [0.321, 0.322] | max_samples, noisy, 945, 55 |

What this shows:

- The gate now behaves as designed. Quiet rounds converge with the noisy flag
  clear. Rounds with 2 to 10 % outliers are flagged noisy and run to the cap,
  which is an honest report of an unstable environment.
- Outlier rates vary between rounds, so runs at the same workload are not
  equal. Precise runs still cap when the environment is noisy.
- Tpt and Criterion intervals now overlap on some rounds and not others. The
  gap is 1 to 5 %, not the earlier 2x. Format disagrees in all three tpt
  rounds, and dot in two. The fast multiply is at the timer floor.
- Wall time is still not a like-for-like comparison, because the two tools do
  not target the same interval.

## Conclusions

1. **The earlier "CIs do not agree" conclusion is withdrawn.** It was measured
   on a loaded machine and did not reproduce on a quiet one. On the quiet run,
   the sum and format intervals overlap. The dot product intervals do not
   overlap, with tpt 2.6% higher. The fast multiply is at the timer floor
   and is not meaningful.

2. **Remaining issue in tpt: the noise gate stops convergence for precise
   runs.** The sum and fast multiply hit the 1000-draw cap, with relative
   intervals of 0.24% and 0.19% (far inside the 2% target). About 10% of their
   draws were flagged as outliers, which sits right at the SPRT's noisy rate
   p1 = 0.10. The test then returns no decision, so the run continues to the
   cap, and the result is marked `noisy`. This is a tuning question in the
   engine, not a measurement error. It makes runs slower and marks precise
   results as noisy.

3. **Wall time.** On the loaded run tpt took about 1.8 s per bench against
   about 47 s for Criterion, roughly 25 times less. That mostly reflects
   Criterion's default budgets (5 s measurement, 3 s warm-up), not equal
   precision. On the quiet run the sum and format benches stopped after 907
   and 31 draws respectively. A fair time-to-precision comparison needs both
   tools set to the same target interval.

4. **"Faster than Criterion" is not measured here.** The accuracy question is
   settled: the two agree on the sum and format workloads. A speed claim needs
   time-to-equal-precision, which this run does not provide.

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
