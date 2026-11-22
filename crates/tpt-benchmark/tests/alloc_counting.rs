//! Integration tests for the allocation layers. This binary installs the
//! counting allocator process-wide, so it lives apart from the unit tests.

use std::hint::black_box;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use tpt_benchmark::MeasurementLayer;
use tpt_benchmark::alloc::{AllocBytes, Allocations, CountingAllocator, thread_dealloc_totals};

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator::new();

/// Runs `f` between the start and end of both allocation layers and returns
/// `(calls, bytes)` for the calling thread.
fn measure(f: impl FnOnce()) -> (f64, f64) {
    let mut calls = Allocations::try_new().expect("counting allocator installed");
    let mut bytes = AllocBytes::try_new().expect("counting allocator installed");
    calls.start_batch();
    bytes.start_batch();
    f();
    let c = calls.end_batch().count;
    let b = bytes.end_batch().bytes;
    (c, b)
}

#[test]
fn zero_alloc_function_reports_exactly_zero() {
    let data = [1_u64, 2, 3, 4];
    let (calls, bytes) = measure(|| {
        let sum: u64 = data.iter().map(|x| x * 2).sum();
        black_box(sum);
    });
    assert_eq!(calls, 0.0);
    assert_eq!(bytes, 0.0);
}

#[test]
fn vec_with_capacity_is_one_call_of_128_bytes() {
    let (calls, bytes) = measure(|| {
        black_box(Vec::<u64>::with_capacity(16));
    });
    assert_eq!(calls, 1.0);
    assert_eq!(bytes, (16 * std::mem::size_of::<u64>()) as f64);
}

#[test]
fn box_u64_is_one_call_of_8_bytes() {
    let (calls, bytes) = measure(|| {
        black_box(Box::new(1_u64));
    });
    assert_eq!(calls, 1.0);
    assert_eq!(bytes, 8.0);
}

#[test]
fn string_from_hello_is_one_call_of_5_bytes() {
    let (calls, bytes) = measure(|| {
        black_box(String::from("hello"));
    });
    assert_eq!(calls, 1.0);
    assert_eq!(bytes, 5.0);
}

#[test]
fn realloc_growth_counts_as_an_allocation_call() {
    // `with_capacity(1)` is one allocation; `reserve(100)` on an empty Vec<u8>
    // grows it to exactly 100 bytes with one realloc.
    let (calls, bytes) = measure(|| {
        let mut v: Vec<u8> = Vec::with_capacity(1);
        v.reserve(100);
        black_box(&v);
    });
    assert_eq!(calls, 2.0);
    assert_eq!(bytes, 101.0);
}

#[test]
fn deallocations_are_counted_separately() {
    let (before_calls, before_bytes) = thread_dealloc_totals();
    drop(black_box(Vec::<u8>::with_capacity(32)));
    let (after_calls, after_bytes) = thread_dealloc_totals();
    assert_eq!(after_calls - before_calls, 1);
    assert_eq!(after_bytes - before_bytes, 32);
}

#[test]
fn other_threads_allocations_are_not_counted() {
    // A noise thread allocates continuously. The measured thread must still
    // see exactly the allocations it makes itself.
    let stop = Arc::new(AtomicBool::new(false));
    let noise_stop = Arc::clone(&stop);
    let noise = thread::spawn(move || {
        while !noise_stop.load(Ordering::Relaxed) {
            black_box(Box::new([0_u8; 64]));
        }
    });

    let mut total = 0.0;
    for _ in 0..50 {
        let (calls, _) = measure(|| {
            for i in 0..5_u64 {
                black_box(Box::new(i));
            }
        });
        total += calls;
    }
    stop.store(true, Ordering::Relaxed);
    noise.join().expect("noise thread panicked");
    assert_eq!(total, 50.0 * 5.0);
}

#[test]
fn each_thread_measures_its_own_count() {
    const THREADS: u64 = 4;
    let barrier = Arc::new(Barrier::new(THREADS as usize));
    let handles: Vec<_> = (0..THREADS)
        .map(|i| {
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                let expected = ((i + 1) * 10) as f64;
                let (calls, bytes) = measure(|| {
                    for _ in 0..(i + 1) * 10 {
                        black_box(Box::new(i));
                    }
                });
                (calls, bytes, expected)
            })
        })
        .collect();

    for handle in handles {
        let (calls, bytes, expected) = handle.join().expect("worker panicked");
        assert_eq!(calls, expected);
        assert_eq!(bytes, expected * 8.0);
    }
}
