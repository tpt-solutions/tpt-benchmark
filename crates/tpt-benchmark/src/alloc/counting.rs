//! The counting global allocator and its thread-local totals.
//!
//! This is the only allocation-related code that uses `unsafe`: the
//! `GlobalAlloc` impl forwards every call unchanged to [`System`].

#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::thread::LocalKey;

thread_local! {
    // `const` initialisers with no destructor mean the slot is never lazily
    // allocated and never registered for destruction, so touching it from
    // inside the allocator cannot recurse into the allocator.
    static ALLOC_CALLS: Cell<u64> = const { Cell::new(0) };
    static ALLOC_BYTES: Cell<u64> = const { Cell::new(0) };
    static DEALLOC_CALLS: Cell<u64> = const { Cell::new(0) };
    static DEALLOC_BYTES: Cell<u64> = const { Cell::new(0) };
}

/// A global allocator that counts calls and bytes on top of [`System`].
///
/// Install it once in the benchmark binary (or test binary):
///
/// ```ignore
/// #[global_allocator]
/// static ALLOC: tpt_benchmark::alloc::CountingAllocator =
///     tpt_benchmark::alloc::CountingAllocator::new();
/// ```
///
/// Counts are kept per thread, so a benchmark measures only the thread that
/// runs its batch. See `docs/allocations.md`.
#[derive(Clone, Copy, Debug, Default)]
pub struct CountingAllocator;

impl CountingAllocator {
    /// Creates the allocator. Usable in a `static` initialiser.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

/// Cumulative allocation totals for the calling thread.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Totals {
    /// `alloc`, `alloc_zeroed` and `realloc` calls that succeeded.
    pub(crate) calls: u64,
    /// Bytes requested by those calls (`realloc` counts its new size).
    pub(crate) bytes: u64,
}

/// Reads the calling thread's cumulative totals.
///
/// Returns zeros if the slots are unreadable, which only happens during
/// thread teardown and only for a thread that never allocated.
pub(crate) fn thread_totals() -> Totals {
    Totals {
        calls: read(&ALLOC_CALLS),
        bytes: read(&ALLOC_BYTES),
    }
}

/// Reads the calling thread's cumulative deallocation totals as `(calls, bytes)`.
pub(crate) fn thread_dealloc_totals() -> (u64, u64) {
    (read(&DEALLOC_CALLS), read(&DEALLOC_BYTES))
}

fn read(key: &'static LocalKey<Cell<u64>>) -> u64 {
    key.try_with(Cell::get).unwrap_or(0)
}

/// Adds to a counter. Never panics and never allocates.
#[inline]
fn bump(key: &'static LocalKey<Cell<u64>>, by: u64) {
    let _ = key.try_with(|c| c.set(c.get().wrapping_add(by)));
}

#[inline]
fn record_alloc(size: u64) {
    bump(&ALLOC_CALLS, 1);
    bump(&ALLOC_BYTES, size);
}

// SAFETY: every method forwards its arguments unchanged to `System`, which
// upholds the `GlobalAlloc` contract. The counter updates touch only
// const-initialised `Cell`s and never allocate, so they cannot re-enter the
// allocator or change the returned pointer.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: the caller guarantees `layout` is non-zero-sized, which is
        // exactly `System::alloc`'s precondition.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size() as u64);
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: same precondition as `alloc`, forwarded unchanged.
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            record_alloc(layout.size() as u64);
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the caller guarantees `ptr` was returned by this allocator
        // with `layout`, which is `System::dealloc`'s precondition.
        unsafe { System.dealloc(ptr, layout) };
        bump(&DEALLOC_CALLS, 1);
        bump(&DEALLOC_BYTES, layout.size() as u64);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: the caller guarantees `ptr` was allocated with `layout` and
        // that `new_size` is non-zero and does not overflow when rounded to
        // `layout.align()`, which is `System::realloc`'s precondition.
        let new_ptr = unsafe { System.realloc(ptr, layout, new_size) };
        if !new_ptr.is_null() {
            record_alloc(new_size as u64);
        }
        new_ptr
    }
}
