# Statistical method

This document describes what the code does today, with the source of each rule.
The implementation is in `crates/tpt-benchmark/src/engine.rs` (decisions),
`src/stats.rs` (arithmetic) and `src/bencher.rs` (the loop that calls them).
If this document and the code disagree, the code is right and this document
needs fixing.

## Inputs and defaults

Per-iteration values are `f64`. For the wall-clock layer they are nanoseconds.
Every value is the layer's batch total divided by the batch's iteration count.

| Parameter | Default | Where |
|-----------|---------|-------|
| `warmup` | 100 ms | `Config` |
| `min_batch` | 1 ms | `Config` |
| `max_batch_iters` | 2^24 | `Config` |
| `rel_error` | 0.02 (±2 %) | `SequentialEngine` |
| `min_samples` | 10 | `SequentialEngine` |
| `max_samples` | 1000 draws | `SequentialEngine` |
| `max_time` | 5 s of sampling | `SequentialEngine` |
| `outlier_k` | 5.0 scaled MADs | `SequentialEngine` |
| SPRT `p0`, `p1` | 0.02, 0.20 | `Sprt` |
| SPRT `alpha`, `beta` | 0.05, 0.05 | `Sprt` |

The runner's `--quick` flag changes three of these: warm-up 20 ms,
`rel_error` 0.05, `max_time` 1 s.

## Loop: warm-up and calibration

1. Warm-up runs the routine in batches of one iteration until `warmup` has
   elapsed. Nothing is recorded.
2. Calibration starts at 10 iterations per batch. The count doubles until one
   batch takes at least `min_batch`, or until `max_batch_iters` is reached.
   The count is then fixed for every sample in the run.

Dividing a batch total by a large iteration count keeps the timer's resolution
and per-call overhead small relative to the measured quantity.

## Running statistics: Welford

Mean and variance of the accepted values come from Welford's online update
(`Welford::push`):

```text
delta = x - mean
mean  = mean + delta / n
m2    = m2 + delta * (x - mean)
variance = m2 / (n - 1)          (unbiased; 0 when n < 2)
```

Welford is used instead of a running sum of squares because it does not lose
precision when the mean is large compared with the spread. The unit test
`welford_is_stable_with_large_offset` checks this with an offset of 10^12.

## Outlier rejection: MAD

Before a candidate is accepted, `is_outlier` compares it with the accepted
values so far:

- **Burn-in.** No candidate is tested until 20 values have been accepted
  (`MIN_SAMPLES_FOR_MAD`). Until then every candidate is accepted and its flag
  is `false`. The reason is that a median and MAD computed from a handful of
  values are unstable. Early ordinary values would then be flagged, and two
  or three false flags are enough for the SPRT (below) to call a clean run
  noisy. Note that the burn-in is 20 accepted values, which is more than
  `min_samples` (10). Precision stopping can therefore fire before outlier
  rejection has started, but it still requires the SPRT to read the flags as
  clean. The draws taken during burn-in are recorded as clean flags.
- **Scale.** With `med` the median of the accepted values:

  ```text
  scale = max(1.4826 * median(|x - med|),  0.005 * |med|)
  ```

  1.4826 makes the MAD a consistent estimate of the standard deviation for
  normal data. The 0.5 % floor exists because timer quantisation can make the
  MAD exactly 0. Without the floor, a value one tick away from identical
  values would be rejected.
- **Rule.** A candidate is an outlier when `|candidate - med| > 5.0 * scale`.
  Rejection starts once 20 values are accepted. Before that the MAD is too
  noisy to use, so no draw is flagged.
  If the scale is zero, NaN or infinite, nothing is rejected.

The median and MAD use only accepted values, so a rejected value does not move
the reference. Every draw still gets a flag, and the flags are what the SPRT
reads.

## Precision stopping: t-interval

Let `n` be the number of accepted values, `s²` their unbiased variance and
`mean` their mean. The 95 % half-width is

```text
h = t(n - 1) * sqrt(s² / n)
```

`t` is the two-sided 95 % Student-t critical value from a 30-entry table for
1 to 30 degrees of freedom (`T_95` in `stats.rs`). For more than 30 degrees of
freedom the code uses 1.96. With fewer than two values the half-width is
infinite, and `t(0)` is infinite.

Precision is met when

```text
h / |mean| <= rel_error          (default 0.02)
```

and it is never met when the mean is zero, NaN or infinite. See the limitation
at the end of this section.

The reported `ci_low` and `ci_high` are `mean ± h`.

## Noise regime: SPRT

Wald's sequential probability ratio test runs on the stream of outlier flags
for all draws, in arrival order. It is re-evaluated after every draw.

- H0 (clean): outlier rate `p0 = 0.02`.
- H1 (noisy): outlier rate `p1 = 0.20`.
- Each outlier adds `ln(p1 / p0) = ln 10 ≈ 2.303` to the log likelihood ratio.
  Each non-outlier adds `ln(0.8 / 0.98) ≈ -0.2029`.
- Lower boundary `ln(beta / (1 - alpha)) = ln(0.05 / 0.95) ≈ -2.944`. The
  stream is decided **Clean** when the sum reaches it.
- Upper boundary `ln((1 - beta) / alpha) = ln(19) ≈ 2.944`. The stream is
  decided **Noisy** when the sum reaches it.
- Otherwise the result is **Undecided**.

On a stream with no outliers, the SPRT decides Clean after 15 scored draws.
A stream with an outlier rate above about 8 % drifts upward and is decided
Noisy in time. Draws in the burn-in period (the first 20) are not scored, so
they cannot decide the verdict. A rate between p0 and p1 leaves the verdict
Undecided, and the run then stops at its caps. The unit tests cover a 33 % flagged stream (Noisy), a clean stream of
300 draws (Clean) and a 2-draw stream (Undecided).

The `noisy` flag in the summary is `true` whenever the verdict is not Clean.
So a run that ends **Undecided** is reported as noisy.

## Stop rules

`SequentialEngine::should_stop` checks these in order, after every batch:

1. **Converged.** `accepted >= min_samples`, precision is met, and the SPRT
   verdict is Clean.
2. **MaxSamples.** Total draws (accepted plus rejected) `>= max_samples`.
3. **MaxTime.** Wall time spent sampling `>= max_time`.

Otherwise sampling continues. Because the caps are checked only after a batch
finishes, a run can overrun `max_time` by up to one batch. The stop reason is
stored in `Summary::stop_reason`.

Converged always implies `noisy == false`, because the SPRT must be Clean.

### Behaviour the caps imply

- A run with a very wide spread cannot meet the precision target. It ends at
  `MaxSamples` or `MaxTime`.
- **A layer whose value is always zero never converges.** The mean is zero,
  so precision is never met, and the run ends at `MaxSamples` (1000 draws),
  or at `MaxTime` if that cap comes first. A zero-allocation routine therefore
  reports `MaxSamples`, not `Converged`, even though its variance is zero.
  This is a property of the current code and affects the zero-allocation use
  case. See `allocations.md` for how the allocation layers are used. It was
  reproduced with a throwaway test whose layer always reports 0 ns.

## What is not implemented: the Bayesian stopping rule

`todo.md` lists a Bayesian stopping rule in Phase 1, which is not done. The
engine uses the precision rule above instead. The reason is as follows:

- The precision rule is a frequentist check: the 95 % Student-t interval must
  be narrower than the target. Its numbers are the same as the 95 % credible
  interval for the mean under the standard noninformative prior
  `p(mean, sigma²) ∝ 1/sigma²`. That is an improper prior, and this is the
  correspondence the rule relies on. It is not a flat prior on `sigma`.
- A Bayesian stopping rule (for example, stopping when the posterior
  probability that the mean lies within ±2 % exceeds a threshold) needs a
  choice of prior and threshold that the project has not settled. It also
  needs validation of its false-stop rate. The SPRT validation test covers
  the noise gate only.
- Until the Bayesian rule exists and is validated, the documentation should
  not describe the stop rule as Bayesian. The Phase 1 item stays unchecked.

## Validation in the test suite

- `ci_coverage_is_close_to_nominal`: coverage of the t-interval on 30-value
  Gaussian samples over 2000 trials lies in 93 % to 97 %.
- `clean_gaussian_streams_are_not_flagged_as_noisy`: on Gaussian data the
  SPRT flags at most 15 of 200 streams as noisy.
- `heavy_outlier_stream_is_flagged_noisy`: a stream with a quarter of its
  values drawn from a wide uniform is flagged Noisy.
- `ci_half_width_shrinks_as_one_over_root_n`, `mad_is_shift_and_scale_equivariant`
  and the other property tests in `stats.rs` use 200 seeded inputs.
- The stress tests (`crates/tpt-benchmark/tests/stress.rs`) check that very
  fast, very slow and noisy routines terminate, and state the stop reason they
  reach.
