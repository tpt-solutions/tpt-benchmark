# Timing overhead

Measured with `cargo run --release -p tpt-benchmark --example overhead` on
Windows 11 (x86_64, invariant TSC). Numbers vary by CPU and OS, so rerun the
example on your target before relying on them.

| Clock    | One read | Amortised per iteration, N = 1 | N = 10  | N = 1 000 | N = 100 000 |
|----------|----------|--------------------------------|---------|-----------|-------------|
| TSC      | ~6.5 ns  | 8.8 ns                         | 0.95 ns | 0.14 ns   | 0.11 ns     |
| Instant  | ~27 ns   | 26 ns                          | 2.6 ns  | 0.14 ns   | 0.11 ns     |

The batched loop divides one start/end read pair by N. The calibration
(`min_batch`, 1 ms by default) grows N until a batch takes at least that long,
so in practice N is large and the overhead is well under 1 ns per iteration.
The floor of about 0.11 ns is the cost of the empty loop itself.

The TSC is used only when CPUID reports an invariant TSC and the calibrated
rate agrees with the wall clock. Otherwise the layer falls back to `Instant`.
