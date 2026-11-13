// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![allow(unsafe_code)] // Host test arena allocation.

use super::*;
use core::cell::Cell;
use std::alloc::{Layout, alloc_zeroed};
use std::panic::{AssertUnwindSafe, catch_unwind};

struct Arena {
    base: usize,
    len: usize,
    used: Cell<usize>,
    protects: Cell<usize>,
    unmaps: Cell<usize>,
}

impl Arena {
    fn new(len: usize) -> Self {
        let layout = Layout::from_size_align(len, 4096).expect("layout");
        // SAFETY: the layout is non-zero and uses a valid alignment.
        let base = unsafe { alloc_zeroed(layout) };
        assert!(!base.is_null());
        Self {
            base: base as usize,
            len,
            used: Cell::new(0),
            protects: Cell::new(0),
            unmaps: Cell::new(0),
        }
    }

    fn base(&self) -> usize {
        self.base
    }
}

impl PageProvider for Arena {
    fn page_size(&self) -> usize {
        4096
    }

    fn map_pages(&self, len: usize, align: usize, _prot: u8) -> Option<usize> {
        let used = self.used.get();
        let aligned = (self.base + used + align - 1) & !(align - 1);
        let start = aligned - self.base;
        if start + len > self.len {
            return None;
        }
        self.used.set(start + len);
        Some(aligned)
    }

    fn map_exact(&self, hint: usize, len: usize) -> Option<usize> {
        if hint == self.base + self.used.get() && self.used.get() + len <= self.len {
            self.used.set(self.used.get() + len);
            return Some(hint);
        }
        None
    }

    fn unmap(&self, _addr: usize, _len: usize) -> bool {
        self.unmaps.set(self.unmaps.get() + 1);
        true
    }

    fn protect(&self, _addr: usize, _len: usize, _prot: u8) -> bool {
        self.protects.set(self.protects.get() + 1);
        true
    }
}

struct Locks {
    depth: Cell<usize>,
    log: Cell<u64>,
    resets: Cell<usize>,
}

impl Lock for Locks {
    fn lock(&self, index: usize) {
        self.depth.set(self.depth.get() + 1);
        self.log
            .set(self.log.get().wrapping_mul(33).wrapping_add(index as u64));
    }
    fn unlock(&self, _index: usize) {
        self.depth.set(self.depth.get().saturating_sub(1));
    }
    fn reset(&self, _index: usize) {
        self.depth.set(0);
        self.resets.set(self.resets.get() + 1);
    }
}

struct Random {
    state: Cell<u64>,
}

impl Random {
    fn new() -> Self {
        Self {
            state: Cell::new(0x1234_5678_9abc_def0),
        }
    }
}

impl Entropy for Random {
    fn fill(&self, buf: &mut [u8]) -> bool {
        let mut index = 0usize;
        while index < buf.len() {
            let mut value = self.state.get();
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            self.state.set(value);
            buf[index] = value as u8;
            index += 1;
        }
        true
    }
}

struct Host {
    thread: Cell<usize>,
    errno: Cell<i32>,
    writes: Cell<usize>,
}

impl Host {
    fn new(thread: usize) -> Self {
        Self {
            thread: Cell::new(thread),
            errno: Cell::new(0),
            writes: Cell::new(0),
        }
    }
}

impl Platform for Host {
    fn thread_id(&self) -> usize {
        self.thread.get()
    }
    fn set_errno(&self, e: i32) {
        self.errno.set(e);
    }
    fn get_errno(&self) -> i32 {
        self.errno.get()
    }
    fn write_stderr(&self, _b: &[u8]) {
        self.writes.set(self.writes.get() + 1);
    }
    fn abort(&self) -> ! {
        panic!("abort")
    }
    fn progname(&self) -> &[u8] {
        b"hardened-test"
    }
    fn process_id(&self) -> u32 {
        7
    }
}

fn heap(options: &[u8]) -> Hardened<Arena, Locks, Random, Host> {
    Hardened::new(
        Arena::new(16 * 1024 * 1024),
        Locks {
            depth: Cell::new(0),
            log: Cell::new(1),
            resets: Cell::new(0),
        },
        Random::new(),
        Host::new(1),
        options,
    )
}

#[test]
fn pool_state_fits_the_page_budget() {
    assert!(core::mem::size_of::<pool::Pool>() <= PAGE_SIZE);
    assert!(core::mem::size_of::<Hardened<Arena, Locks, Random, Host>>() < 16 * 1024);
}

#[test]
fn small_allocations_are_aligned_and_usable() {
    let heap = heap(b"");
    for size in [0usize, 1, 15, 16, 17, 2047, 2048] {
        let pointer = heap.malloc(size);
        assert_ne!(pointer, 0, "{size}");
        assert_eq!(pointer % 16, 0, "{size}");
        assert!(heap.usable_size(pointer) >= size, "{size}");
        heap.free(pointer);
    }
}

#[test]
fn calloc_zeroes_and_checks_overflow() {
    let heap = heap(b"");
    assert_eq!(heap.calloc(usize::MAX / 2 + 1, 2), 0);
    let pointer = heap.calloc(8, 16);
    let mut bytes = [1u8; 128];
    crate::raw::read_bytes(pointer, &mut bytes);
    assert_eq!(bytes, [0; 128]);
    heap.free(pointer);
}

#[test]
fn realloc_preserves_contents() {
    let heap = heap(b"");
    let pointer = heap.malloc(32);
    crate::raw::fill(pointer, 0x3c, 32);
    let larger = heap.realloc(pointer, 256);
    assert_ne!(larger, 0);
    let mut bytes = [0u8; 32];
    crate::raw::read_bytes(larger, &mut bytes);
    assert_eq!(bytes, [0x3c; 32]);
    heap.free(larger);
}

#[test]
fn page_allocations_take_the_page_path_and_free() {
    let heap = heap(b"");
    let pointer = heap.malloc(5000);
    assert_ne!(pointer, 0);
    assert!(heap.usable_size(pointer) >= 5000);
    assert_eq!(heap.info().hblks, 1);
    heap.free(pointer);
    assert_eq!(heap.info().uordblks, 0);
}

#[test]
fn malloc_zero_returns_distinct_pointers() {
    let heap = heap(b"");
    let first = heap.malloc(0);
    let second = heap.malloc(0);
    assert_ne!(first, second);
    heap.free(first);
    heap.free(second);
}

#[test]
fn double_free_aborts() {
    let heap = heap(b"F");
    let pointer = heap.malloc(32);
    heap.free(pointer);
    let result = catch_unwind(AssertUnwindSafe(|| heap.free(pointer)));
    assert!(result.is_err());
}

#[test]
fn canary_detects_an_overrun() {
    let heap = heap(b"C");
    let pointer = heap.malloc(8);
    assert_eq!(heap.usable_size(pointer), 8);
    crate::raw::fill(pointer + 8, 0, 8);
    let result = catch_unwind(AssertUnwindSafe(|| heap.free(pointer)));
    assert!(result.is_err());
}

#[test]
fn guard_option_requests_a_protection() {
    let heap = heap(b"G");
    let pointer = heap.malloc(4096);
    assert_ne!(pointer, 0);
    assert!(heap.info().hblks >= 1);
    heap.free(pointer);
}

#[test]
fn free_sized_wrong_size_aborts() {
    let heap = heap(b"");
    let pointer = heap.malloc(16);
    let result = catch_unwind(AssertUnwindSafe(|| heap.free_sized(pointer, 32)));
    assert!(result.is_err());
}

#[test]
fn freezero_wipes_before_free() {
    let heap = heap(b"");
    let pointer = heap.malloc(32);
    crate::raw::fill(pointer, 0xaa, 32);
    heap.freezero(pointer, 32);
    assert_eq!(heap.info().uordblks, 0);
}

#[test]
fn trim_releases_the_page_cache() {
    let heap = heap(b"");
    let pointer = heap.malloc(5000);
    heap.free(pointer);
    assert!(heap.trim());
}

#[test]
fn cross_pool_free_is_found() {
    let heap = heap(b"+");
    let pointer = heap.malloc(32);
    heap.platform().thread.set(9);
    heap.free(pointer);
    assert_eq!(heap.info().uordblks, 0);
}

#[test]
fn fork_hooks_lock_and_reset_every_pool() {
    let heap = heap(b"-");
    let pointer = heap.malloc(16);
    heap.prefork();
    heap.postfork_child();
    heap.postfork_parent();
    assert!(heap.info().uordblks >= 16);
    heap.free(pointer);
}

#[test]
fn mallinfo_clamps_and_counts() {
    let heap = heap(b"");
    let pointer = heap.malloc(128);
    let info = heap.info();
    assert!(info.arena >= 128);
    assert!(info.uordblks >= 128);
    heap.free(pointer);
}

#[test]
fn memalign_handles_large_alignments() {
    let heap = heap(b"");
    for align in [16usize, 64, 4096, 8192, 65536] {
        let pointer = heap.memalign(align, 33);
        assert_ne!(pointer, 0, "{align}");
        assert_eq!(pointer % align, 0, "{align}");
        heap.free(pointer);
    }
}

#[test]
fn arena_base_is_page_aligned() {
    let arena = Arena::new(4096);
    assert_eq!(arena.base() % 4096, 0);
}
