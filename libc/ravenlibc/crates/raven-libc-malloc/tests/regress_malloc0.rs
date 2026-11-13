// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! OpenBSD `malloc0test`: zero-size allocations are unique and writable.

#![allow(clippy::undocumented_unsafe_blocks)]

mod common;

use common::{SpinLock, TestEntropy, TestPlatform, VecProvider};
use raven_libc_malloc::{Hardened, Heap};

#[test]
fn zero_size_allocations_are_distinct_and_do_not_corrupt_neighbours() {
    let heap = Hardened::new(
        VecProvider::new(),
        SpinLock::new(),
        TestEntropy::new(1),
        TestPlatform::new(1),
        b"",
    );
    let mut pointers = Vec::new();
    for _ in 0..64 {
        let pointer = heap.malloc(0);
        assert_ne!(pointer, 0);
        unsafe { core::ptr::write_bytes(pointer as *mut u8, 0x41, 1) };
        pointers.push(pointer);
    }
    for pair in pointers.windows(2) {
        assert_ne!(pair[0], pair[1]);
    }
    for pointer in pointers {
        heap.free(pointer);
    }
}
