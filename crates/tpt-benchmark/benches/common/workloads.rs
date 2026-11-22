//! Workloads shared by `vs_criterion` and `vs_tpt`. Both benches include this
//! file, so the measured routines are byte-for-byte identical.

use std::hint::black_box;

/// Side length of the square f64 matrices in the dot-product workload.
pub const MATRIX_SIDE: usize = 128;

/// Number of iterations of the string-building loop.
pub const FORMAT_ITERS: u64 = 100;

/// Input for [`sum_u64`]: 1024 deterministic values.
pub fn sum_input() -> Vec<u64> {
    (0..1024_u64).collect()
}

/// Sums a slice of `u64`. The input is passed through `black_box`.
pub fn sum_u64(values: &[u64]) -> u64 {
    black_box(values).iter().sum()
}

/// Inputs for [`dot_f64`]: two 128x128 matrices, flattened row-major.
pub fn dot_input() -> (Vec<f64>, Vec<f64>) {
    let n = MATRIX_SIDE * MATRIX_SIDE;
    let a: Vec<f64> = (0..n).map(|i| (i % 97) as f64 * 0.5).collect();
    let b: Vec<f64> = (0..n).map(|i| (i % 89) as f64 * 0.25).collect();
    (a, b)
}

/// Dot product of two equally sized flattened 128x128 f64 matrices.
pub fn dot_f64(a: &[f64], b: &[f64]) -> f64 {
    black_box(a)
        .iter()
        .zip(black_box(b))
        .map(|(x, y)| x * y)
        .sum()
}

/// Builds a `String` with 100 `format!` calls. Allocates on every iteration.
pub fn build_string() -> String {
    let mut out = String::new();
    for i in 0..FORMAT_ITERS {
        out.push_str(&format!("{}-{}", i, i * 2));
    }
    out
}

/// Fixed input for [`fast_mul`].
pub const FAST_INPUT: u64 = 0x9E37_79B9;

/// A routine of about 5 ns, used to stress timer overhead.
pub fn fast_mul(x: u64) -> u64 {
    black_box(x).wrapping_mul(3)
}
