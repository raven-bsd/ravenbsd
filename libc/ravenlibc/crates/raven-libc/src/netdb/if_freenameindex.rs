// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `if_freenameindex` from `<net/if.h>`.

use core::ffi::c_void;

use super::if_names::CIfNameIndex;

/// `if_freenameindex`.
///
/// # Safety
/// `pointer` is NULL or the result of `if_nameindex`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn if_freenameindex(pointer: *mut CIfNameIndex) {
    if pointer.is_null() {
        return;
    }
    // SAFETY: the pointer is the one block from `if_nameindex`.
    unsafe { crate::malloc::free::free(pointer.cast::<c_void>()) };
}
