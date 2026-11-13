// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A recording heap for the compatibility unit tests.

use core::cell::Cell;

use crate::compat::{Heap, Mallinfo2};

/// A heap that returns one internal buffer and records calls.
pub struct TestHeap {
    buffer: Cell<[u8; 256]>,
    pub freed: Cell<usize>,
    pub realloc_result: Cell<usize>,
    pub malloc_result: Cell<usize>,
}

impl TestHeap {
    /// A heap whose `malloc` returns its internal buffer.
    pub fn new() -> Self {
        let heap = Self {
            buffer: Cell::new([0; 256]),
            freed: Cell::new(0),
            realloc_result: Cell::new(0),
            malloc_result: Cell::new(0),
        };
        heap.malloc_result
            .set(heap.buffer.as_ptr() as *mut u8 as usize);
        heap
    }

    /// The internal buffer address.
    pub fn address(&self) -> usize {
        self.buffer.as_ptr() as *mut u8 as usize
    }
}

impl Default for TestHeap {
    fn default() -> Self {
        Self::new()
    }
}

impl Heap for TestHeap {
    fn malloc(&self, _size: usize) -> usize {
        self.malloc_result.get()
    }
    fn free(&self, p: usize) {
        self.freed.set(p);
    }
    fn realloc(&self, _p: usize, _size: usize) -> usize {
        self.realloc_result.get()
    }
    fn calloc(&self, _n: usize, _m: usize) -> usize {
        self.malloc_result.get()
    }
    fn memalign(&self, _align: usize, _size: usize) -> usize {
        self.malloc_result.get()
    }
    fn usable_size(&self, _p: usize) -> usize {
        256
    }
    fn trim(&self) -> bool {
        false
    }
    fn info(&self) -> Mallinfo2 {
        Mallinfo2::default()
    }
    fn free_sized(&self, p: usize, _size: usize) {
        self.free(p);
    }
    fn free_aligned_sized(&self, p: usize, _align: usize, _size: usize) {
        self.free(p);
    }
    fn prefork(&self) {}
    fn postfork_parent(&self) {}
    fn postfork_child(&self) {}
}
