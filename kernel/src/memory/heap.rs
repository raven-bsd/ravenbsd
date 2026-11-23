// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Kernel heap.
//!
//! A size-class allocator (`raven-kmem`) over a `raven-vmem` arena spanning the architecture's
//! `KERNEL_HEAP_BASE`. The initial 4 MiB are mapped from fresh frames; allocation failures grow
//! the heap by 1 MiB and import the newly mapped range into the arena before retrying. The
//! arena and allocator hold only fixed inline metadata, so allocation never recurses into the
//! global allocator. The allocator never panics: it returns null and the stable runtime raises
//! its allocation-error panic.

use core::alloc::{GlobalAlloc, Layout};
use core::ptr;

use spin::Mutex;

#[cfg(target_arch = "aarch64")]
use raven_abi::layout::aarch64::{KERNEL_HEAP_BASE, KERNEL_HEAP_END};
#[cfg(target_arch = "x86_64")]
use raven_abi::layout::x86_64::{KERNEL_HEAP_BASE, KERNEL_HEAP_END};
use raven_kmem::KmemGeneric;
use raven_vmem::VmemGeneric;

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

/// Boundary tags in the arena (roughly two per 4 KiB slab, so this bounds the total heap).
const TAGS: usize = 8192;
/// Slabs per size class (bounds the live objects each class can serve).
const MAX_SLABS: usize = 256;

struct State {
    heap: KmemGeneric<MAX_SLABS, TAGS>,
    mapped: u64,
}

static HEAP: Mutex<Option<State>> = Mutex::new(None);

/// The global kernel allocator.
pub struct Allocator;

#[global_allocator]
static GLOBAL: Allocator = Allocator;

/// Map the initial heap and install the allocator.
pub fn init() -> Result<(), paging::PagingError> {
    if HEAP.lock().is_some() {
        return Ok(());
    }
    paging::map_heap_range(0, INITIAL_HEAP_SIZE)?;
    // The arena and the size-class allocator hold only inline storage, so construction cannot
    // fail for the valid, non-empty kernel heap range.
    let arena = VmemGeneric::<TAGS>::new(KERNEL_HEAP_BASE, INITIAL_HEAP_SIZE)
        .expect("kernel heap range is valid");
    let heap = KmemGeneric::new(arena).expect("kmem construction is infallible");
    *HEAP.lock() = Some(State {
        heap,
        mapped: INITIAL_HEAP_SIZE,
    });
    kprintln!("heap: {} KiB", INITIAL_HEAP_SIZE >> 10);
    Ok(())
}

/// Heap statistics as `(used, free)` bytes at the arena level.
pub fn stats() -> (usize, usize) {
    let guard = HEAP.lock();
    match guard.as_ref() {
        Some(state) => (
            state.heap.arena().total_allocated() as usize,
            state.heap.arena().total_free() as usize,
        ),
        None => (0, 0),
    }
}

/// K449 diagnostic: `(in_use, slabs, alloc_fail)` for one general size class.
pub fn class_stats(class: usize) -> Option<(usize, usize, u64)> {
    let guard = HEAP.lock();
    let stats = guard.as_ref()?.heap.class_stats(class)?;
    Some((stats.in_use, stats.slabs, stats.alloc_fail))
}

/// Run the `selftest=heap` workload.
pub fn selftest() {
    {
        let boxes: alloc::vec::Vec<alloc::boxed::Box<[u8; 4096]>> = (0..200)
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

/// Grow the heap by [`GROW_SIZE`], importing the freshly mapped range into the arena.
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
    let base = KERNEL_HEAP_BASE.saturating_add(state.mapped);
    if state.heap.arena_mut().import(base, GROW_SIZE).is_err() {
        return false;
    }
    state.mapped = next;
    true
}

// SAFETY: `alloc` returns null on failure and only hands out memory from the locked allocator;
// `dealloc` returns exactly pointers produced by `alloc` to the same allocator. All access to the
// non-Sync allocator is serialized by the spin mutex, and the allocator's own metadata is inline,
// so no allocation re-enters this lock.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            // A zero-size allocation still needs a non-null, aligned address.
            return layout.align() as *mut u8;
        }
        let mut guard = HEAP.lock();
        let Some(state) = guard.as_mut() else {
            return ptr::null_mut();
        };
        loop {
            match state.heap.alloc(layout.size(), layout.align()) {
                Ok(address) => return address as *mut u8,
                Err(_) => {
                    if !grow(state) {
                        return ptr::null_mut();
                    }
                }
            }
        }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if pointer.is_null() || layout.size() == 0 {
            return;
        }
        let mut guard = HEAP.lock();
        if let Some(state) = guard.as_mut() {
            let _ = state
                .heap
                .free(pointer as usize as u64, layout.size(), layout.align());
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
