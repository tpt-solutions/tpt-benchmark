//! Criterion side of the tpt-benchmark vs Criterion comparison.
//!
//! Run with `cargo bench -p tpt-benchmark --bench vs_criterion`. Results land
//! in `target/criterion/<name>/new/estimates.json`. The workloads are shared
//! with `vs_tpt` through `common/workloads.rs`.

#[path = "common/workloads.rs"]
mod workloads;

use std::hint::black_box;

use criterion::{Criterion, criterion_group, criterion_main};

fn vs_criterion(c: &mut Criterion) {
    let sum_input = workloads::sum_input();
    c.bench_function("sum_u64_1024", |b| {
        b.iter(|| workloads::sum_u64(&sum_input));
    });

    let (a, m) = workloads::dot_input();
    c.bench_function("dot_f64_128x128", |b| {
        b.iter(|| workloads::dot_f64(&a, &m));
    });

    c.bench_function("format_string_100", |b| {
        b.iter(workloads::build_string);
    });

    c.bench_function("fast_mul_u64", |b| {
        b.iter(|| workloads::fast_mul(black_box(workloads::FAST_INPUT)));
    });
}

criterion_group!(benches, vs_criterion);
criterion_main!(benches);
