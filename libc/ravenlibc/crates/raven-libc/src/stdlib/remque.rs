// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `remque(3)`.
use core::ffi::c_void;

use super::search_support::RawListStore;

/// Remove `element` from its list.
///
/// # Safety
/// `element` is NULL or points to an object linked by `insque`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn remque(element: *mut c_void) {
    if element.is_null() {
        return;
    }
    let mut store = RawListStore;
    raven_libc_stdlib::search::remque(&mut store, element as usize);
}
