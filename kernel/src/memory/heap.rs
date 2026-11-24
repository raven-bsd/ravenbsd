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
use core::sync::atomic::{AtomicUsize, Ordering};

use spin::Mutex;

/// Failed allocation size and caller addresses (K434 owner diagnostic).
static FAIL_SIZE: AtomicUsize = AtomicUsize::new(0);
static FAIL_ALIGN: AtomicUsize = AtomicUsize::new(0);
static FAIL_RA0: AtomicUsize = AtomicUsize::new(0);
static FAIL_RA1: AtomicUsize = AtomicUsize::new(0);
/// Failed allocation caller frame chain (K434 object-site diagnostic).
const FAIL_BT_DEPTH: usize = 6;
static FAIL_BT: [AtomicUsize; FAIL_BT_DEPTH] = [
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
];

/// Failed allocation `(size, align, caller_return_addresses)`; size 0 when none.
pub fn fail_info() -> (usize, usize, usize, usize) {
    (
        FAIL_SIZE.load(Ordering::Acquire),
        FAIL_ALIGN.load(Ordering::Acquire),
        FAIL_RA0.load(Ordering::Acquire),
        FAIL_RA1.load(Ordering::Acquire),
    )
}

/// Caller return addresses captured when the last allocation failed.
pub fn fail_backtrace() -> [usize; FAIL_BT_DEPTH] {
    [
        FAIL_BT[0].load(Ordering::Acquire),
        FAIL_BT[1].load(Ordering::Acquire),
        FAIL_BT[2].load(Ordering::Acquire),
        FAIL_BT[3].load(Ordering::Acquire),
        FAIL_BT[4].load(Ordering::Acquire),
        FAIL_BT[5].load(Ordering::Acquire),
    ]
}

fn record_fail(size: usize, align: usize, ra0: usize, ra1: usize) {
    FAIL_SIZE.store(size, Ordering::Release);
    FAIL_ALIGN.store(align, Ordering::Release);
    FAIL_RA0.store(ra0, Ordering::Release);
    FAIL_RA1.store(ra1, Ordering::Release);
    #[cfg(target_arch = "x86_64")]
    {
        // `ra1` is the saved frame pointer of the allocation shim. Walk the caller chain so the
        // panic handler can name the object site that requested the failed size. The bounds keep
        // a corrupted frame from driving the walk into unmapped memory on the panic path.
        let mut frame = ra1;
        let mut depth = 0usize;
        while depth < FAIL_BT_DEPTH && frame != 0 {
            // SAFETY: frame is a kernel-stack frame pointer published by the compiler; the
            // monotonic bound below rejects an invalid next frame before it is dereferenced.
            let next = unsafe { core::ptr::read_volatile(frame as *const usize) };
            // SAFETY: [frame + 8] is the return address slot of the frame at `frame`.
            let ret = unsafe { core::ptr::read_volatile(frame.saturating_add(8) as *const usize) };
            if let Some(slot) = FAIL_BT.get(depth) {
                slot.store(ret, Ordering::Release);
            }
            if next <= frame || next.saturating_sub(frame) > 0x1_0000 {
                break;
            }
            frame = next;
            depth = depth.saturating_add(1);
        }
    }
}

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
/// Last heap-growth failure: 0 none, 1 limit, 2 map, 3 import exhausted, 4 import no tags.
static GROW_FAIL: AtomicUsize = AtomicUsize::new(0);
/// Failed frees and the layout of the last rejected block (K434 diagnostic).
static FREE_FAILS: AtomicUsize = AtomicUsize::new(0);
static FREE_FAIL_SIZE: AtomicUsize = AtomicUsize::new(0);
static FREE_FAIL_ALIGN: AtomicUsize = AtomicUsize::new(0);
static FREE_FAIL_PTR: AtomicUsize = AtomicUsize::new(0);
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

/// Statistics for one general size-class slot (S36b diagnostic).
pub fn class_stats(index: usize) -> Option<raven_kmem::CacheStats> {
    let guard = HEAP.lock();
    guard.as_ref()?.heap.general_stats(index)
}

/// Last heap-growth failure reason code.
pub fn grow_fail() -> usize {
    GROW_FAIL.load(Ordering::Acquire)
}

/// Global kmem counters `(alloc_fail, large_live, in_use_objects)`.
pub fn kmem_counters() -> Option<(u64, u64, u64)> {
    let guard = HEAP.lock();
    let stats = guard.as_ref()?.heap.stats();
    Some((
        stats.alloc_fail,
        stats.large_live,
        u64::try_from(stats.in_use).unwrap_or(0),
    ))
}

/// Failed frees `(count, last_size, last_align, last_pointer)`.
pub fn free_fail_info() -> (usize, usize, usize, usize) {
    (
        FREE_FAILS.load(Ordering::Acquire),
        FREE_FAIL_SIZE.load(Ordering::Acquire),
        FREE_FAIL_ALIGN.load(Ordering::Acquire),
        FREE_FAIL_PTR.load(Ordering::Acquire),
    )
}

/// Allocated-segment histogram by size: `<=1K, <=2K, <=4K, <=8K, <=16K, <=64K, <=1M, >1M`.
pub fn segment_histogram() -> Option<([u64; 8], [u64; 8])> {
    let guard = HEAP.lock();
    let arena = guard.as_ref()?.heap.arena();
    let mut counts = [0u64; 8];
    let mut bytes = [0u64; 8];
    for segment in arena.segments().filter(|segment| !segment.is_free()) {
        let size = segment.size();
        let index = if size <= 1024 {
            0
        } else if size <= 2048 {
            1
        } else if size <= 4096 {
            2
        } else if size <= 8192 {
            3
        } else if size <= 16 * 1024 {
            4
        } else if size <= 64 * 1024 {
            5
        } else if size <= 1024 * 1024 {
            6
        } else {
            7
        };
        if let Some(slot) = counts.get_mut(index) {
            *slot = slot.saturating_add(1);
        }
        if let Some(slot) = bytes.get_mut(index) {
            *slot = slot.saturating_add(size);
        }
    }
    Some((counts, bytes))
}

/// Arena counters `(spans, free_bytes, largest_free, alloc_fail, used_bytes)`.
pub fn arena_stats() -> Option<(usize, u64, u64, u64, u64)> {
    let guard = HEAP.lock();
    let stats = guard.as_ref()?.heap.arena().stats();
    Some((
        stats.spans,
        stats.free_bytes,
        stats.largest_free,
        stats.alloc_fail,
        stats.used_bytes,
    ))
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
        GROW_FAIL.store(1, Ordering::Release);
        return false;
    };
    if KERNEL_HEAP_BASE.saturating_add(next) > HEAP_LIMIT {
        GROW_FAIL.store(1, Ordering::Release);
        return false;
    }
    if paging::map_heap_range(state.mapped, GROW_SIZE).is_err() {
        GROW_FAIL.store(2, Ordering::Release);
        return false;
    }
    let base = KERNEL_HEAP_BASE.saturating_add(state.mapped);
    if let Err(error) = state.heap.arena_mut().import(base, GROW_SIZE) {
        GROW_FAIL.store(
            if error == raven_vmem::VmemError::NoTags {
                4
            } else {
                3
            },
            Ordering::Release,
        );
        return false;
    }
    state.mapped = next;
    GROW_FAIL.store(0, Ordering::Release);
    true
}

// SAFETY: `alloc` returns null on failure and only hands out memory from the locked allocator;
// `dealloc` returns exactly pointers produced by `alloc` to the same allocator. All access to the
// non-Sync allocator is serialized by the spin mutex, and the allocator's own metadata is inline,
// so no allocation re-enters this lock.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let (mut ra0, mut ra1) = (0usize, 0usize);
        #[cfg(target_arch = "x86_64")]
        unsafe {
            // The allocator is built with a frame pointer: `[rbp + 8]` is this call's return
            // address and `[rbp]` the caller's frame pointer.
            core::arch::asm!(
                "mov {}, [rbp + 8]",
                out(reg) ra0,
                options(nomem, nostack, preserves_flags)
            );
            core::arch::asm!(
                "mov {}, [rbp]",
                out(reg) ra1,
                options(nomem, nostack, preserves_flags)
            );
        }
        if layout.size() == 0 {
            // A zero-size allocation still needs a non-null, aligned address.
            return layout.align() as *mut u8;
        }
        let mut guard = HEAP.lock();
        let Some(state) = guard.as_mut() else {
            record_fail(layout.size(), layout.align(), ra0, ra1);
            return ptr::null_mut();
        };
        loop {
            match state.heap.alloc(layout.size(), layout.align()) {
                Ok(address) => return address as *mut u8,
                Err(_) => {
                    if !grow(state) {
                        record_fail(layout.size(), layout.align(), ra0, ra1);
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
            if state
                .heap
                .free(pointer as usize as u64, layout.size(), layout.align())
                .is_err()
            {
                FREE_FAILS.fetch_add(1, Ordering::AcqRel);
                FREE_FAIL_SIZE.store(layout.size(), Ordering::Release);
                FREE_FAIL_ALIGN.store(layout.align(), Ordering::Release);
                FREE_FAIL_PTR.store(pointer as usize, Ordering::Release);
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
