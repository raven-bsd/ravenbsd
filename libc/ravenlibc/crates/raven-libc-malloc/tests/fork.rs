// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fork-hook lock order and post-fork usability.

mod common;

use common::{SpinLock, TestEntropy, TestPlatform, VecProvider};
use raven_libc_malloc::{Hardened, Heap, Tiny};

#[test]
fn hardened_fork_hooks_leave_the_heap_usable() {
    let heap = Hardened::new(
        VecProvider::new(),
        SpinLock::new(),
        TestEntropy::new(8),
        TestPlatform::new(1),
        b"+",
    );
    let pointer = heap.malloc(32);
    heap.prefork();
    heap.postfork_child();
    heap.postfork_parent();
    heap.free(pointer);
    let second = heap.malloc(64);
    assert_ne!(second, 0);
    heap.free(second);
}

#[test]
fn tiny_fork_hooks_leave_the_heap_usable() {
    let heap = Tiny::new(VecProvider::new(), SpinLock::new(), TestPlatform::new(1));
    let pointer = heap.malloc(32);
    heap.prefork();
    heap.postfork_child();
    heap.postfork_parent();
    heap.free(pointer);
    let second = heap.malloc(64);
    assert_ne!(second, 0);
    heap.free(second);
}
