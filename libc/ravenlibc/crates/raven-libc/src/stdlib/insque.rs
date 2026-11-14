// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `insque(3)`.
use core::ffi::c_void;

use super::search_support::RawListStore;

/// Insert `entry` after `pred`, or start a one-element list.
///
/// # Safety
/// Both pointers are NULL or point to the caller's at-least-two-pointer objects.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn insque(entry: *mut c_void, pred: *mut c_void) {
    if entry.is_null() {
        return;
    }
    let mut store = RawListStore;
    let node = entry as usize;
    let predecessor = if pred.is_null() {
        None
    } else {
        Some(pred as usize)
    };
    raven_libc_stdlib::search::insque(&mut store, node, predecessor);
}
