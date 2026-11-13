// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Alignment matrix on both backends.

#![allow(clippy::undocumented_unsafe_blocks)]

mod common;

use common::{SpinLock, TestEntropy, TestPlatform, VecProvider};
use raven_libc_malloc::{Hardened, Heap, Tiny};

fn check(heap: &dyn Heap, align: usize, size: usize) {
    let pointer = heap.memalign(align, size);
    assert_ne!(pointer, 0, "align {align} size {size}");
    assert_eq!(pointer % align, 0, "align {align} size {size}");
    let usable = heap.usable_size(pointer);
    assert!(usable >= size, "align {align} size {size} usable {usable}");
    if usable != 0 {
        unsafe { core::ptr::write_bytes(pointer as *mut u8, 0x5a, usable) };
    }
    heap.free(pointer);
}

#[test]
fn hardened_alignment_matrix() {
    let heap = Hardened::new(
        VecProvider::new(),
        SpinLock::new(),
        TestEntropy::new(11),
        TestPlatform::new(1),
        b"",
    );
    let aligns = [1usize, 2, 8, 16, 64, 4096, 8192, 65536, 1 << 21];
    let sizes = [
        0usize,
        1,
        15,
        16,
        17,
        2047,
        2048,
        2049,
        4095,
        4096,
        4097,
        65535,
        65536,
        1 << 20,
    ];
    for align in aligns {
        for size in sizes {
            check(&heap, align, size);
        }
    }
}

#[test]
fn tiny_alignment_matrix() {
    let heap = Tiny::new(VecProvider::new(), SpinLock::new(), TestPlatform::new(1));
    let aligns = [1usize, 2, 8, 16, 64, 4096, 8192, 65536, 1 << 21];
    let sizes = [
        0usize,
        1,
        15,
        16,
        17,
        2047,
        2048,
        2049,
        4095,
        4096,
        4097,
        65535,
        65536,
        1 << 20,
    ];
    for align in aligns {
        for size in sizes {
            check(&heap, align, size);
        }
    }
}
