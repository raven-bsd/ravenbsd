// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A minimal target global allocator for the `alloc`-backed C52 views.
//!
//! The production loader uses `ldalloc` for its own objects; this bump region satisfies the
//! `Vec` allocations inside `raven-libc-elf`. It is replaced by the pager-backed `ldalloc`
//! when PG-13 lands.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};

const POOL: usize = 1 << 20;

#[repr(align(16))]
struct Pool(UnsafeCell<[u8; POOL]>);

// SAFETY: the bump pointer serialises all accesses; the pool is only handed out once.
unsafe impl Sync for Pool {}

static POOL_DATA: Pool = Pool(UnsafeCell::new([0; POOL]));
static NEXT: AtomicUsize = AtomicUsize::new(0);

/// The target allocator.
pub struct LdGlobalAlloc;

// SAFETY: each allocation advances the atomic bump offset; `dealloc` is a no-op, which is
// correct for a loader that never frees C52's short-lived views.
unsafe impl GlobalAlloc for LdGlobalAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(16);
        let start = NEXT.load(Ordering::Relaxed);
        let mask = align.wrapping_sub(1);
        let aligned = start
            .checked_add(mask)
            .map(|value| value & !mask)
            .unwrap_or(usize::MAX);
        let end = aligned.saturating_add(layout.size());
        if end > POOL {
            return core::ptr::null_mut();
        }
        NEXT.store(end, Ordering::Relaxed);
        // SAFETY: `aligned..end` lies inside the static pool.
        unsafe { (POOL_DATA.0.get() as *mut u8).add(aligned) }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[global_allocator]
static ALLOC: LdGlobalAlloc = LdGlobalAlloc;
