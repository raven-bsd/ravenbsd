// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A counting global allocator for the peak-allocation property.
//!
//! This is the single `unsafe` module the ticket allows: `unsafe impl GlobalAlloc` with a
//! `// SAFETY:` note. It is used only by this crate's tests and bins; no shipped crate links it.
//! The counters are thread-local so parallel tests do not perturb one another.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static PEAK: Cell<usize> = const { Cell::new(0) };
    static MARK: Cell<usize> = const { Cell::new(0) };
    static NOTE: Cell<u64> = const { Cell::new(0) };
}

/// The allocator that records live and peak bytes.
pub struct CountingAlloc;

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

// SAFETY: every method forwards the exact layout to `System`; the counters are only
// bookkeeping and never influence the pointer returned by `alloc`/`realloc`.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: caller upholds the `GlobalAlloc` contract; `System` has the same contract.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            record_alloc(layout.size());
        }
        pointer
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        // SAFETY: caller upholds the `GlobalAlloc` contract; `System` has the same contract.
        let pointer = unsafe { System.alloc_zeroed(layout) };
        if !pointer.is_null() {
            record_alloc(layout.size());
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        record_dealloc(layout.size());
        // SAFETY: `pointer` came from this allocator and `layout` is its layout.
        unsafe { System.dealloc(pointer, layout) };
    }

    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // SAFETY: caller upholds the `GlobalAlloc` contract; `System` has the same contract.
        let new_pointer = unsafe { System.realloc(pointer, layout, new_size) };
        if !new_pointer.is_null() {
            record_dealloc(layout.size());
            record_alloc(new_size);
        }
        new_pointer
    }
}

fn record_alloc(size: usize) {
    LIVE.with(|live| {
        let value = live.get().saturating_add(size);
        live.set(value);
        PEAK.with(|peak| {
            if value > peak.get() {
                peak.set(value);
            }
        });
    });
}

fn record_dealloc(size: usize) {
    LIVE.with(|live| live.set(live.get().saturating_sub(size)));
}

/// Record the live-byte mark and start a new peak window.
pub fn mark() -> usize {
    let live = LIVE.with(Cell::get);
    MARK.with(|mark| mark.set(live));
    PEAK.with(|peak| peak.set(live));
    live
}

/// Peak bytes above the last [`mark`].
#[must_use]
pub fn peak_since_mark() -> usize {
    PEAK.with(Cell::get).saturating_sub(MARK.with(Cell::get))
}

/// Current live bytes.
#[must_use]
pub fn live() -> usize {
    LIVE.with(Cell::get)
}

/// Record one observation hash. The last value wins; a body calls it once at the end.
pub fn note(value: u64) {
    NOTE.with(|note| note.set(value));
}

/// The last recorded observation hash.
#[must_use]
pub fn last_note() -> u64 {
    NOTE.with(Cell::get)
}

/// Clear the note (tests compare two runs).
pub fn clear_note() {
    NOTE.with(|note| note.set(0));
}

#[cfg(test)]
#[path = "alloc_probe_ut.rs"]
mod tests;
