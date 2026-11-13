// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The Rust global allocator over the C49 heap, for `alloc`-using code inside the archive and
//! for Rust rlib consumers (C60 4.3 item 8: one allocator).

use core::alloc::{GlobalAlloc, Layout};

use crate::malloc::heap;

/// The global allocator handle.
struct GlobalHeap;

// SAFETY: every method forwards to the C49 front end, whose contract is the C allocator
// contract; the layout size/alignment are validated by it.
unsafe impl GlobalAlloc for GlobalHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        heap::api().malloc(layout.size()) as *mut u8
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        heap::api().calloc(1, layout.size()) as *mut u8
    }

    unsafe fn dealloc(&self, pointer: *mut u8, _layout: Layout) {
        heap::api().free(pointer as usize);
    }

    unsafe fn realloc(&self, pointer: *mut u8, _layout: Layout, new_size: usize) -> *mut u8 {
        heap::api().realloc(pointer as usize, new_size) as *mut u8
    }
}

#[global_allocator]
static GLOBAL: GlobalHeap = GlobalHeap;
