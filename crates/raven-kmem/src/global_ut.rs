// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `GlobalAlloc` shim, including the byte-preserving hammer.
//!
//! The shim hands out addresses from a caller-provided arena. On the host those addresses are not
//! mapped, so each test backs the arena with a real heap buffer whose address range the arena
//! manages; writing through a returned pointer then lands inside that buffer.

use super::*;
use core::alloc::{GlobalAlloc, Layout};
use raven_vmem::VmemGeneric;

/// The shim used by these tests: enough slabs and tags for the byte-preserving hammer.
type TestAlloc = KmemAllocGeneric<64, 512>;
type TestKmem = KmemGeneric<64, 512>;

/// Build a shim whose arena covers a real heap buffer of `bytes` bytes. The buffer is returned
/// alongside the shim so it stays alive while the shim is used.
fn shim(bytes: usize) -> (TestAlloc, Box<[u8]>) {
    let backing: Box<[u8]> = alloc::vec![0u8; bytes].into_boxed_slice();
    let base = backing.as_ptr() as usize as u64;
    let arena = VmemGeneric::<512>::new(base, bytes as u64).expect("arena");
    let shim = TestAlloc::new(TestKmem::new(arena).expect("kmem"));
    (shim, backing)
}

/// `GlobalAlloc::alloc` through the shim.
fn shim_alloc(shim: &TestAlloc, layout: Layout) -> *mut u8 {
    // SAFETY: `layout` is a valid layout and the returned pointer, when non-null, is valid for
    // `layout.size()` bytes; the caller releases it with `shim_dealloc`.
    unsafe { GlobalAlloc::alloc(shim, layout) }
}

/// `GlobalAlloc::dealloc` through the shim.
fn shim_dealloc(shim: &TestAlloc, ptr: *mut u8, layout: Layout) {
    // SAFETY: `ptr` was returned by `shim_alloc` for `layout` and not yet freed.
    unsafe { GlobalAlloc::dealloc(shim, ptr, layout) };
}

/// `GlobalAlloc::realloc` through the shim.
fn shim_realloc(shim: &TestAlloc, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
    // SAFETY: `ptr` was returned by `shim_alloc` for `layout` and not yet freed; `new_size` is an
    // ordinary non-negative length.
    unsafe { GlobalAlloc::realloc(shim, ptr, layout, new_size) }
}

/// Fill a block so a later check can detect a missed copy.
fn write_pattern(ptr: *mut u8, size: usize) {
    // SAFETY: the caller passes a pointer valid for `size` writable bytes (one just returned by
    // `shim_alloc` for `size`).
    unsafe {
        for i in 0..size {
            ptr.add(i).write((size as u8).wrapping_add(i as u8));
        }
    }
}

/// Verify a block still holds the pattern written for `tag`.
fn check_pattern(ptr: *mut u8, size: usize, tag: u8) {
    // SAFETY: the caller passes a pointer valid for `size` readable bytes.
    unsafe {
        for i in 0..size {
            assert_eq!(*ptr.add(i), tag.wrapping_add(i as u8), "byte {i} corrupt");
        }
    }
}

#[test]
fn alloc_and_dealloc_round_trip() {
    let (shim, _backing) = shim(0x10_0000);
    let layout = Layout::from_size_align(64, 8).expect("layout");
    let ptr = shim_alloc(&shim, layout);
    assert!(!ptr.is_null());
    assert_eq!(ptr as usize % 8, 0);
    write_pattern(ptr, 64);
    check_pattern(ptr, 64, 64);
    shim_dealloc(&shim, ptr, layout);
    assert_eq!(shim.stats().allocs, 1);
    assert_eq!(shim.stats().frees, 1);
}

#[test]
fn zero_size_alloc_is_non_null_and_aligned() {
    let (shim, _backing) = shim(0x1000);
    let layout = Layout::from_size_align(0, 8).expect("layout");
    let ptr = shim_alloc(&shim, layout);
    assert!(!ptr.is_null());
    assert_eq!(ptr as usize % 8, 0);
    shim_dealloc(&shim, ptr, layout);
}

#[test]
fn realloc_growth_moves_and_preserves_bytes() {
    let (shim, _backing) = shim(0x10_0000);
    let layout = Layout::from_size_align(16, 8).expect("layout");
    let ptr = shim_alloc(&shim, layout);
    write_pattern(ptr, 16);

    let new_size = 128; // a different size class, so it must move
    let new_ptr = shim_realloc(&shim, ptr, layout, new_size);
    assert!(!new_ptr.is_null());
    assert_ne!(new_ptr, ptr);
    check_pattern(new_ptr, 16, 16);
    shim_dealloc(
        &shim,
        new_ptr,
        Layout::from_size_align(new_size, 8).expect("layout"),
    );
}

#[test]
fn realloc_shrink_in_place() {
    let (shim, _backing) = shim(0x10_0000);
    let layout = Layout::from_size_align(64, 8).expect("layout");
    let ptr = shim_alloc(&shim, layout);
    // 49 still rounds to the 64-byte class, so the shrink stays in place.
    let new_ptr = shim_realloc(&shim, ptr, layout, 49);
    assert_eq!(new_ptr, ptr, "same size class stays in place");
    shim_dealloc(
        &shim,
        new_ptr,
        Layout::from_size_align(49, 8).expect("layout"),
    );
}

#[test]
fn realloc_to_zero_frees() {
    let (shim, _backing) = shim(0x10_0000);
    let layout = Layout::from_size_align(64, 8).expect("layout");
    let ptr = shim_alloc(&shim, layout);
    let new_ptr = shim_realloc(&shim, ptr, layout, 0);
    assert!(!new_ptr.is_null());
    assert_eq!(shim.stats().frees, 1);
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit
    }
}

#[test]
fn global_alloc_hammer() {
    let (shim, _backing) = shim(0x4_0000);
    let mut rng = Rng(0xfeed_beef_cafe_f00d);
    let mut live: Vec<(usize, usize, usize)> = Vec::new();

    for _ in 0..3000 {
        if live.is_empty() || rng.below(100) < 60 {
            let size = 1 + rng.below(2000) as usize;
            let align = 1usize << rng.below(7);
            let layout = Layout::from_size_align(size, align).expect("layout");
            let ptr = shim_alloc(&shim, layout);
            if ptr.is_null() {
                continue;
            }
            assert_eq!(ptr as usize % align, 0);
            write_pattern(ptr, size);
            live.push((ptr as usize, size, align));
        } else if rng.below(100) < 85 {
            let idx = rng.below(live.len() as u64) as usize;
            let (addr, size, align) = live[idx];
            let new_size = 1 + rng.below(2000) as usize;
            let old_layout = Layout::from_size_align(size, align).expect("layout");
            let new_ptr = shim_realloc(&shim, addr as *mut u8, old_layout, new_size);
            if new_ptr.is_null() {
                continue;
            }
            check_pattern(new_ptr, core::cmp::min(size, new_size), size as u8);
            write_pattern(new_ptr, new_size);
            live[idx] = (new_ptr as usize, new_size, align);
        } else {
            let idx = rng.below(live.len() as u64) as usize;
            let (addr, size, align) = live.swap_remove(idx);
            let layout = Layout::from_size_align(size, align).expect("layout");
            shim_dealloc(&shim, addr as *mut u8, layout);
        }
    }

    for (addr, size, align) in live.drain(..) {
        let layout = Layout::from_size_align(size, align).expect("layout");
        shim_dealloc(&shim, addr as *mut u8, layout);
    }

    let stats = shim.stats();
    assert_eq!(stats.frees, stats.allocs);
    assert_eq!(stats.in_use, 0);
    assert_eq!(stats.large_live, 0);
}
