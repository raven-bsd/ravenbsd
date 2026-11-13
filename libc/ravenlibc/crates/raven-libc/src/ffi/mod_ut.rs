// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Placeholder module test: the `ffi` unit tests live in the sibling `_ut.rs` files.

#[test]
fn the_errno_lock_is_constructible() {
    let guard = crate::ffi::TestLock::acquire();
    drop(guard);
}
