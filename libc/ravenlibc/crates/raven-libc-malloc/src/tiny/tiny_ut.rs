// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![allow(unsafe_code)] // Host test arena allocation.

use super::*;
use core::cell::Cell;
use std::alloc::{Layout, alloc_zeroed};

struct Arena {
    base: usize,
    len: usize,
    used: Cell<usize>,
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
            unmaps: Cell::new(0),
        }
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
        true
    }
}

struct Locks {
    locked: Cell<usize>,
}

impl Lock for Locks {
    fn lock(&self, _index: usize) {
        self.locked.set(self.locked.get() + 1);
    }
    fn unlock(&self, _index: usize) {
        self.locked.set(self.locked.get().saturating_sub(1));
    }
    fn reset(&self, _index: usize) {
        self.locked.set(0);
    }
}

struct Host {
    errno: Cell<i32>,
    writes: Cell<usize>,
}

impl crate::provider::Platform for Host {
    fn thread_id(&self) -> usize {
        1
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
        b"tiny-test"
    }
    fn process_id(&self) -> u32 {
        7
    }
}

fn heap() -> Tiny<Arena, Locks, Host> {
    Tiny::new(
        Arena::new(4 * 1024 * 1024),
        Locks {
            locked: Cell::new(0),
        },
        Host {
            errno: Cell::new(0),
            writes: Cell::new(0),
        },
    )
}

#[test]
fn malloc_returns_aligned_zeroed_memory() {
    let heap = heap();
    let pointer = heap.malloc(100);
    assert_ne!(pointer, 0);
    assert_eq!(pointer % 16, 0);
    let mut bytes = [1u8; 100];
    raw::read_bytes(pointer, &mut bytes);
    assert_eq!(bytes, [0; 100]);
}

#[test]
fn malloc_zero_returns_a_unique_minimum_block() {
    let heap = heap();
    let first = heap.malloc(0);
    let second = heap.malloc(0);
    assert_ne!(first, 0);
    assert_ne!(second, 0);
    assert_ne!(first, second);
    heap.free(first);
    heap.free(second);
}

#[test]
fn free_and_reuse_keep_alignment_and_content() {
    let heap = heap();
    let pointer = heap.malloc(64);
    raw::fill(pointer, 0x5a, 64);
    heap.free(pointer);
    let again = heap.malloc(32);
    assert_ne!(again, 0);
    assert_eq!(again % 16, 0);
    let mut bytes = [0u8; 32];
    raw::read_bytes(again, &mut bytes);
    assert_eq!(bytes, [0; 32], "tiny malloc zeroes reused blocks too");
}

#[test]
fn realloc_shrinks_and_grows() {
    let heap = heap();
    let pointer = heap.malloc(32);
    raw::fill(pointer, 0x33, 32);
    let smaller = heap.realloc(pointer, 16);
    assert_ne!(smaller, 0);
    assert!(heap.usable_size(smaller) >= 16);
    let larger = heap.realloc(smaller, 128);
    assert_ne!(larger, 0);
    let mut bytes = [0u8; 16];
    raw::read_bytes(larger, &mut bytes);
    assert_eq!(bytes, [0x33; 16]);
    heap.free(larger);
}

#[test]
fn calloc_checks_overflow_and_zeroes() {
    let heap = heap();
    assert_eq!(heap.calloc(usize::MAX / 2 + 1, 2), 0);
    let pointer = heap.calloc(4, 8);
    let mut bytes = [1u8; 32];
    raw::read_bytes(pointer, &mut bytes);
    assert_eq!(bytes, [0; 32]);
    heap.free(pointer);
}

#[test]
fn memalign_large_size_64() {
    let heap = heap();
    let pointer = heap.memalign(64, 65_535);
    assert_ne!(pointer, 0);
    assert_eq!(pointer % 64, 0);
    heap.free(pointer);
}

#[test]
fn memalign_handles_page_and_large_alignments() {
    let heap = heap();
    for align in [1usize, 2, 8, 16, 64, 4096, 8192, 65536] {
        let pointer = heap.memalign(align, 17);
        assert_ne!(pointer, 0, "{align}");
        assert_eq!(pointer % align, 0, "{align}");
        assert!(heap.usable_size(pointer) >= 17);
        heap.free(pointer);
    }
}

#[cfg(not(feature = "debug"))]
#[test]
fn double_free_sets_enomem_without_debug() {
    let heap = heap();
    let pointer = heap.malloc(16);
    heap.free(pointer);
    heap.free(pointer);
    assert_eq!(heap.platform().errno.get(), raven_libc_abi::errno::ENOMEM);
}

#[test]
fn usable_size_reports_at_least_the_request() {
    let heap = heap();
    for size in [0usize, 1, 15, 16, 17, 2047, 2048, 2049, 4096] {
        let pointer = heap.malloc(size);
        assert!(heap.usable_size(pointer) >= size, "{size}");
        heap.free(pointer);
    }
}

#[test]
fn realloc_growth_preserves_first_bytes_across_free_reuse() {
    let heap = heap();
    // A short buffer (like a short line) is filled, grown, freed, then a fresh allocation
    // reuses the memory; the first bytes must survive the growth.
    let p = heap.malloc(8);
    assert_ne!(p, 0);
    raw::fill(p, 0x5A, 8);
    let q = heap.realloc(p, 128);
    assert_ne!(q, 0);
    let mut bytes = [0u8; 8];
    raw::read_bytes(q, &mut bytes);
    assert_eq!(bytes, [0x5A; 8], "first 8 bytes survive the growth");
    heap.free(q);

    // Shrink-to-zero then grow again, the exact zero-capacity reuse shape.
    let a = heap.malloc(24);
    raw::fill(a, 0x6B, 24);
    assert_eq!(heap.realloc(a, 0), 0, "realloc to 0 frees");
    let b = heap.malloc(24);
    assert_ne!(b, 0);
    let mut bytes = [1u8; 24];
    raw::read_bytes(b, &mut bytes);
    assert_eq!(bytes, [0u8; 24], "reused block is zeroed, not stale");
    raw::fill(b, 0x6C, 24);
    let c = heap.realloc(b, 64);
    let mut bytes = [0u8; 24];
    raw::read_bytes(c, &mut bytes);
    assert_eq!(bytes, [0x6C; 24], "content survives grow after reuse");
    heap.free(c);
}

struct ReallocRng(u64);

impl ReallocRng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, limit: usize) -> usize {
        if limit == 0 {
            0
        } else {
            (self.next() as usize) % limit
        }
    }
}

#[test]
fn realloc_sweep_preserves_content_across_sizes() {
    // A growth-then-check sweep over many sizes, mirroring a String that grows once.
    for initial in 0..512usize {
        let heap = heap();
        let p = heap.malloc(initial);
        assert_ne!(p, 0, "malloc {initial}");
        raw::fill(p, 0x77, initial);
        let q = heap.realloc(p, initial + 64);
        assert_ne!(q, 0, "realloc {initial}");
        let mut offset = 0;
        while offset < initial {
            assert_eq!(
                raw::read_u8(q.wrapping_add(offset)),
                0x77,
                "realloc {initial} lost byte {offset}"
            );
            offset += 1;
        }
        heap.free(q);
    }
}

#[test]
fn repeated_growth_preserves_content() {
    // A String-like amortized growth: alloc once, then realloc several times, checking every
    // byte survives each growth.
    let heap = heap();
    let mut cap = 1usize;
    let mut pointer = heap.malloc(cap);
    assert_ne!(pointer, 0);
    let mut len = 0usize;
    let mut value = 0u8;
    while cap < 4096 {
        // Fill the current buffer up to capacity with a per-byte pattern.
        while len < cap {
            raw::write_u8(pointer.wrapping_add(len), value);
            value = value.wrapping_add(1);
            len += 1;
        }
        let new_cap = cap * 2;
        pointer = heap.realloc(pointer, new_cap);
        assert_ne!(pointer, 0, "realloc to {new_cap}");
        cap = new_cap;
        // Every previously written byte must survive.
        let mut offset = 0;
        while offset < len {
            assert_eq!(
                raw::read_u8(pointer.wrapping_add(offset)),
                (offset as u8).wrapping_mul(1),
                "growth lost byte {offset} at cap {cap}"
            );
            offset += 1;
        }
    }
    heap.free(pointer);
}

#[test]
fn realloc_fuzz_preserves_content_and_never_overlaps() {
    let heap = heap();
    let mut rng = ReallocRng(0x0ddc_0ffe_e123_4567);
    let mut live: Vec<(usize, usize, u8)> = Vec::new(); // (address, size, pattern)
    for step in 0..50_000usize {
        if live.is_empty() || rng.below(100) < 45 {
            let size = rng.below(256);
            let address = heap.malloc(size);
            if address != 0 {
                let pattern = (step as u8).wrapping_add(1);
                if size != 0 {
                    raw::fill(address, pattern, size);
                }
                live.push((address, size, pattern));
            }
        } else if rng.below(100) < 60 {
            // free
            let index = rng.below(live.len());
            let (address, size, pattern) = live.swap_remove(index);
            let mut offset = 0;
            while offset < size {
                assert_eq!(
                    raw::read_u8(address.wrapping_add(offset)),
                    pattern,
                    "block content corrupted before free"
                );
                offset += 1;
            }
            heap.free(address);
        } else {
            // realloc
            let index = rng.below(live.len());
            let (address, size, pattern) = live[index];
            let new_size = rng.below(512);
            if new_size == 0 {
                live.swap_remove(index);
                heap.free(address);
                continue;
            }
            let new_address = heap.realloc(address, new_size);
            if new_address == 0 {
                continue;
            }
            let preserved = size.min(new_size);
            let mut offset = 0;
            while offset < preserved {
                assert_eq!(
                    raw::read_u8(new_address.wrapping_add(offset)),
                    pattern,
                    "realloc lost content at offset {offset}"
                );
                offset += 1;
            }
            // The extension bytes are zeroed by malloc; fill them with the block pattern so the
            // next full-block check is uniform.
            if new_size > size {
                raw::fill(new_address.wrapping_add(size), pattern, new_size - size);
            }
            live[index] = (new_address, new_size, pattern);
        }
    }
    for (address, _, _) in live {
        heap.free(address);
    }
}

#[test]
fn info_tracks_arena_and_free_bytes() {
    let heap = heap();
    let pointer = heap.malloc(128);
    let info = heap.info();
    assert!(info.arena >= 64 * 1024);
    assert!(info.uordblks > 0);
    heap.free(pointer);
    assert!(!heap.trim());
}
