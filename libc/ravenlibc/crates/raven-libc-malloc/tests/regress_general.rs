// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! OpenBSD `malloc_general`: many sizes, both backends and the security option set.

#![allow(clippy::undocumented_unsafe_blocks)]

mod common;

use common::{SpinLock, TestEntropy, TestPlatform, VecProvider};
use raven_libc_malloc::{Hardened, Heap, Tiny};

fn exercise(heap: &dyn Heap) {
    let sizes = [
        0usize, 1, 2, 15, 16, 17, 31, 32, 33, 127, 128, 129, 2047, 2048, 2049, 4095, 4096, 4097,
        65535,
    ];
    for size in sizes {
        let pointer = heap.malloc(size);
        assert_ne!(pointer, 0, "{size}");
        if size != 0 {
            unsafe { core::ptr::write_bytes(pointer as *mut u8, 0xa5, size) };
        }
        assert!(heap.usable_size(pointer) >= size);
        heap.free(pointer);

        let pointer = heap.calloc(size, 2);
        assert_ne!(pointer, 0, "{size}");
        heap.free(pointer);

        let pointer = heap.malloc(size);
        let larger = heap.realloc(pointer, size + 64);
        assert_ne!(larger, 0, "{size}");
        heap.free(larger);
    }
}

#[test]
fn hardened_many_sizes_and_options() {
    for options in [b"".as_slice(), b"S", b"J", b"F"] {
        let heap = Hardened::new(
            VecProvider::new(),
            SpinLock::new(),
            TestEntropy::new(7),
            TestPlatform::new(1),
            options,
        );
        exercise(&heap);
    }
}

#[test]
fn tiny_many_sizes() {
    let heap = Tiny::new(VecProvider::new(), SpinLock::new(), TestPlatform::new(1));
    exercise(&heap);
}
