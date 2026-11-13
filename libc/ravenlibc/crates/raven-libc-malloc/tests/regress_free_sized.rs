// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! C23 `free_sized`/`free_aligned_sized` on both backends.

mod common;

use common::{SpinLock, TestEntropy, TestPlatform, VecProvider};
use raven_libc_malloc::{Hardened, Heap, Tiny};

#[test]
fn tiny_ignores_the_size() {
    let heap = Tiny::new(VecProvider::new(), SpinLock::new(), TestPlatform::new(1));
    for size in [0usize, 1, 16, 4096] {
        let pointer = heap.malloc(size);
        heap.free_sized(pointer, size);
        let pointer = heap.malloc(size);
        heap.free_aligned_sized(pointer, 16, size);
    }
}

#[test]
fn hardened_checks_the_size() {
    let heap = Hardened::new(
        VecProvider::new(),
        SpinLock::new(),
        TestEntropy::new(4),
        TestPlatform::new(1),
        b"",
    );
    for size in [1usize, 16, 17, 2049, 4096] {
        let pointer = heap.malloc(size);
        heap.free_sized(pointer, size);
        let pointer = heap.malloc(size);
        heap.free_aligned_sized(pointer, 16, size);
    }
}
