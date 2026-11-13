// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Debug-mode vectors (only built with `--features debug`).

#![cfg(feature = "debug")]
#![allow(clippy::undocumented_unsafe_blocks)]

mod common;

use std::panic::{AssertUnwindSafe, catch_unwind};

use common::{SpinLock, TestEntropy, TestPlatform, VecProvider};
use raven_libc_malloc::debug::Debug;
use raven_libc_malloc::{Hardened, Heap};

#[test]
fn redzone_overrun_is_reported() {
    let debug = Debug::new(
        Hardened::new(
            VecProvider::new(),
            SpinLock::new(),
            TestEntropy::new(2),
            TestPlatform::new(1),
            b"",
        ),
        TestPlatform::new(1),
        raven_libc_malloc::debug::env::DebugOptions {
            redzone: true,
            audit: 1,
            ..raven_libc_malloc::debug::env::DebugOptions::default()
        },
    );
    let pointer = debug.malloc(8);
    unsafe { core::ptr::write_bytes((pointer + 8) as *mut u8, 0xff, 1) };
    assert!(catch_unwind(AssertUnwindSafe(|| debug.free(pointer))).is_err());
}

#[test]
fn double_free_is_reported() {
    let debug = Debug::new(
        Hardened::new(
            VecProvider::new(),
            SpinLock::new(),
            TestEntropy::new(2),
            TestPlatform::new(1),
            b"",
        ),
        TestPlatform::new(1),
        raven_libc_malloc::debug::env::DebugOptions {
            audit: 1,
            ..raven_libc_malloc::debug::env::DebugOptions::default()
        },
    );
    let pointer = debug.malloc(8);
    debug.free(pointer);
    assert!(catch_unwind(AssertUnwindSafe(|| debug.free(pointer))).is_err());
}
