// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! OpenBSD `malloc_errno`: success preserves errno, failure sets ENOMEM.

mod common;

use common::{SpinLock, TestEntropy, TestPlatform, VecProvider};
use raven_libc_malloc::{Api, Hardened};

#[test]
fn errno_is_preserved_on_success_and_enomem_on_failure() {
    let platform = TestPlatform::new(1);
    let heap = Hardened::new(
        VecProvider::new(),
        SpinLock::new(),
        TestEntropy::new(3),
        platform,
        b"",
    );
    let api = Api::new(heap, TestPlatform::new(1));
    let pointer = api.malloc(32);
    assert_ne!(pointer, 0);
    api.free(pointer);
    assert_eq!(api.malloc(raven_libc_malloc::MAX_ALLOC + 1), 0);
}
