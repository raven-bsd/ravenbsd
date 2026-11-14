// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `unveil(2)`.
use core::ffi::{c_char, c_int};

use crate::personality_common::{RealPledge, UNVEIL_STATE, ret_unit};

fn optional(pointer: *const c_char) -> Option<&'static [u8]> {
    if pointer.is_null() {
        None
    } else {
        // SAFETY: the caller's C string contract; the borrow lives for the call.
        Some(unsafe { crate::ffi::cstr::slice(pointer) })
    }
}

/// Restrict the visible path set.
///
/// # Safety
/// Each argument is NULL or a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unveil(path: *const c_char, perms: *const c_char) -> c_int {
    let path = optional(path);
    let perms = optional(perms);
    ret_unit(UNVEIL_STATE.with(|state| {
        raven_libc_personality_bsd::pledge_glue::unveil(&mut RealPledge, state, path, perms)
    }))
}
