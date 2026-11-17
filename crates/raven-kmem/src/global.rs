// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `GlobalAlloc` shim over a [`Kmem`].

use core::alloc::{GlobalAlloc, Layout};
use core::cell::RefCell;
use core::ptr;

use raven_vmem::DEFAULT_TAG_CAPACITY;

use crate::kmem::{DEFAULT_MAX_SLABS, KmemGeneric, KmemStats};
use crate::size::classify;

/// A [`GlobalAlloc`] backed by a caller-owned [`KmemGeneric`].
///
/// There is no global static and no cross-thread sharing: the shim owns its allocator behind a
/// [`RefCell`] and is driven directly, so it stays single-threaded. This module is the only place
/// in the crate that uses `unsafe`: the [`GlobalAlloc`] methods carry the raw-pointer contract the
/// trait itself defines, and `realloc` copies bytes between two allocations.
pub struct KmemAllocGeneric<const MAX_SLABS: usize, const TAGS: usize> {
    kmem: RefCell<KmemGeneric<MAX_SLABS, TAGS>>,
}

/// The host-tested shim: default slab and tag capacities.
pub type KmemAlloc = KmemAllocGeneric<DEFAULT_MAX_SLABS, DEFAULT_TAG_CAPACITY>;

impl<const MAX_SLABS: usize, const TAGS: usize> KmemAllocGeneric<MAX_SLABS, TAGS> {
    /// Wrap a freshly built allocator as a [`GlobalAlloc`].
    pub fn new(kmem: KmemGeneric<MAX_SLABS, TAGS>) -> Self {
        Self {
            kmem: RefCell::new(kmem),
        }
    }

    /// Snapshot the underlying allocator's statistics.
    pub fn stats(&self) -> KmemStats {
        self.kmem.borrow().stats()
    }

    /// Recover the owned allocator.
    pub fn into_inner(self) -> KmemGeneric<MAX_SLABS, TAGS> {
        self.kmem.into_inner()
    }
}

// SAFETY: the shim owns its allocator behind a `RefCell` and is driven by a single thread (there is
// no global static and no sharing), so `borrow_mut` cannot panic and the single-threaded invariant
// the `GlobalAlloc` trait assumes holds. Every returned pointer is an address inside the
// caller-provided arena and is released with a matching `dealloc`.
unsafe impl<const MAX_SLABS: usize, const TAGS: usize> GlobalAlloc
    for KmemAllocGeneric<MAX_SLABS, TAGS>
{
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if layout.size() == 0 {
            // A zero-size allocation still needs a non-null, aligned address.
            return layout.align() as *mut u8;
        }
        match self.kmem.borrow_mut().alloc(layout.size(), layout.align()) {
            Ok(address) => address as *mut u8,
            Err(_) => ptr::null_mut(),
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() || layout.size() == 0 {
            return;
        }
        let _ = self
            .kmem
            .borrow_mut()
            .free(ptr as usize as u64, layout.size(), layout.align());
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if new_size == 0 {
            // SAFETY: `ptr` and `layout` are the caller's original allocation, so `dealloc` may
            // free them.
            unsafe {
                self.dealloc(ptr, layout);
            }
            return layout.align() as *mut u8;
        }
        let Ok(new_layout) = Layout::from_size_align(new_size, layout.align()) else {
            return ptr::null_mut();
        };
        if ptr.is_null() || layout.size() == 0 {
            // SAFETY: `new_layout` is a valid layout constructed above, so `alloc` may be called
            // with it; there is nothing to copy from the null or zero-sized source.
            return unsafe { self.alloc(new_layout) };
        }
        // In-place when the old and new sizes share the same size class (and neither is large).
        let old_class = classify(layout.size(), layout.align());
        if old_class.is_some() && old_class == classify(new_size, layout.align()) {
            return ptr;
        }
        // SAFETY: `new_layout` is valid; a returned null pointer is handled below.
        let new_ptr = unsafe { self.alloc(new_layout) };
        if new_ptr.is_null() {
            return ptr::null_mut();
        }
        // SAFETY: `ptr` is valid for `layout.size()` bytes (caller contract) and `new_ptr` is
        // valid for `new_size` bytes (returned by `alloc` above); the copy length is the smaller
        // of the two, so both ranges cover it.
        unsafe {
            ptr::copy_nonoverlapping(ptr, new_ptr, core::cmp::min(layout.size(), new_size));
        }
        // SAFETY: `ptr` and `layout` are the original allocation, no longer needed after the copy.
        unsafe {
            self.dealloc(ptr, layout);
        }
        new_ptr
    }
}

#[cfg(test)]
#[path = "global_ut.rs"]
mod tests;
