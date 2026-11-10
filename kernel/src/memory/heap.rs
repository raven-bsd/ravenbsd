// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel heap.
//!
//! A linked-list allocator over a contiguous virtual range starting at the architecture's
//! `KERNEL_HEAP_BASE` (the same constant on x86-64 and aarch64). The initial 4 MiB are mapped
//! from fresh frames; allocation failures grow the heap by 1 MiB and retry. The allocator never
//! panics: it returns null and the stable runtime raises its allocation-error panic.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

use linked_list_allocator::Heap;
#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::{KERNEL_HEAP_BASE, KERNEL_HEAP_END};
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::{KERNEL_HEAP_BASE, KERNEL_HEAP_END};
use spin::Mutex;

use crate::kprintln;
use crate::memory::paging;

/// Initial heap size.
const INITIAL_HEAP_SIZE: u64 = 4 * 1024 * 1024;
/// Growth step.
const GROW_SIZE: u64 = 1024 * 1024;
/// The MMIO bump window starts here inside the heap region.
pub const MMIO_WINDOW_OFFSET: u64 = 0x1000_0000_0000;
/// Heap growth stops before the MMIO window.
const HEAP_LIMIT: u64 = KERNEL_HEAP_BASE.saturating_add(MMIO_WINDOW_OFFSET);

struct State {
    heap: Heap,
    mapped: u64,
}

static HEAP: Mutex<Option<State>> = Mutex::new(None);

/// The global kernel allocator.
pub struct Allocator;

#[global_allocator]
static GLOBAL: Allocator = Allocator;

/// Map the initial heap.
pub fn init() -> Result<(), paging::PagingError> {
    if HEAP.lock().is_some() {
        return Ok(());
    }
    paging::map_heap_range(0, INITIAL_HEAP_SIZE)?;
    // SAFETY: the range was just mapped RW+NX at `KERNEL_HEAP_BASE` and is not used elsewhere.
    let heap = unsafe {
        Heap::new(
            KERNEL_HEAP_BASE as *mut u8,
            usize::try_from(INITIAL_HEAP_SIZE).map_err(|_| paging::PagingError::Overflow)?,
        )
    };
    *HEAP.lock() = Some(State {
        heap,
        mapped: INITIAL_HEAP_SIZE,
    });
    kprintln!("heap: {} KiB", INITIAL_HEAP_SIZE >> 10);
    Ok(())
}

/// Heap statistics as `(used, free)` bytes.
pub fn stats() -> (usize, usize) {
    let guard = HEAP.lock();
    match guard.as_ref() {
        Some(state) => (state.heap.used(), state.heap.free()),
        None => (0, 0),
    }
}

/// Run the `selftest=heap` workload.
pub fn selftest() {
    {
        let boxes: alloc::vec::Vec<alloc::boxed::Box<[u8; 4096]>> = (0..1000)
            .map(|_| alloc::boxed::Box::new([0u8; 4096]))
            .collect();
        drop(boxes);

        let vector: alloc::vec::Vec<u8> = alloc::vec![0u8; 8 * 1024 * 1024];
        drop(vector);

        let mut map: alloc::collections::BTreeMap<u64, u64> = alloc::collections::BTreeMap::new();
        for value in 0..10_000u64 {
            map.insert(value, value.wrapping_mul(3));
        }
        drop(map);
    }
    let (used, free) = stats();
    kprintln!("selftest: heap ok used={} free={}", used, free);
}

fn grow(state: &mut State) -> bool {
    let Some(next) = state.mapped.checked_add(GROW_SIZE) else {
        return false;
    };
    if KERNEL_HEAP_BASE.saturating_add(next) > HEAP_LIMIT {
        return false;
    }
    if paging::map_heap_range(state.mapped, GROW_SIZE).is_err() {
        return false;
    }
    // SAFETY: `map_heap_range` mapped the next `GROW_SIZE` bytes contiguously after the current
    // heap, and `state.mapped` is only advanced after the mapping succeeds.
    unsafe {
        state.heap.extend(usize::try_from(GROW_SIZE).unwrap_or(0));
    }
    state.mapped = next;
    true
}

// SAFETY: `alloc` returns null on failure and only hands out memory from the locked heap;
// `dealloc` returns exactly pointers produced by `alloc` to the same allocator. All access to the
// non-Sync `Heap` is serialized by the spin mutex.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let mut guard = HEAP.lock();
        let Some(state) = guard.as_mut() else {
            return ptr::null_mut();
        };
        loop {
            match state.heap.allocate_first_fit(layout) {
                Ok(pointer) => return pointer.as_ptr(),
                Err(()) => {
                    if !grow(state) {
                        return ptr::null_mut();
                    }
                }
            }
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        let Some(non_null) = core::ptr::NonNull::new(pointer) else {
            return;
        };
        let mut guard = HEAP.lock();
        if let Some(state) = guard.as_mut() {
            // SAFETY: the caller passes a live pointer from this allocator with the same layout.
            unsafe {
                state.heap.deallocate(non_null, layout);
            }
        }
    }
}

const _: () = {
    assert!(INITIAL_HEAP_SIZE == 4 * 1024 * 1024);
    assert!(GROW_SIZE == 1024 * 1024);
    assert!(MMIO_WINDOW_OFFSET == 0x1000_0000_0000);
    assert!(KERNEL_HEAP_BASE < HEAP_LIMIT);
    assert!(HEAP_LIMIT <= KERNEL_HEAP_END);
};
