// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! NetBSD `t_posix_memalign` cases on both backends.

mod common;

use common::{SpinLock, TestEntropy, TestPlatform, VecProvider};
use raven_libc_malloc::{Api, Hardened, Heap, Tiny};

#[test]
fn posix_memalign_cases_on_tiny() {
    let api = Api::new(
        Tiny::new(VecProvider::new(), SpinLock::new(), TestPlatform::new(1)),
        TestPlatform::new(1),
    );
    let mut out = 0usize;
    assert_eq!(api.posix_memalign(&mut out, 1, 8), 22);
    assert_eq!(api.posix_memalign(&mut out, 3, 8), 22);
    assert_eq!(api.posix_memalign(&mut out, 4, 8), 22);
    assert_eq!(api.posix_memalign(&mut out, 8, 8), 0);
    assert_eq!(out % 8, 0);
    api.free(out);
    assert_eq!(api.posix_memalign(&mut out, 4096, 4097), 0);
    assert_eq!(out % 4096, 0);
    assert!(api.heap().usable_size(out) >= 4097);
    api.free(out);
}

#[test]
fn posix_memalign_cases_on_hardened() {
    let heap = Hardened::new(
        VecProvider::new(),
        SpinLock::new(),
        TestEntropy::new(5),
        TestPlatform::new(1),
        b"",
    );
    let api = Api::new(heap, TestPlatform::new(1));
    let mut out = 0usize;
    assert_eq!(api.posix_memalign(&mut out, 0, 8), 22);
    assert_eq!(api.posix_memalign(&mut out, 16, 0), 0);
    api.free(out);
    assert_eq!(api.posix_memalign(&mut out, 8192, 33), 0);
    assert_eq!(out % 8192, 0);
    api.free(out);
}
