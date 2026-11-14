// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `pledge(2)`.
use core::ffi::{c_char, c_int};

use crate::personality_common::{PLEDGE_STATE, RealPledge, ret_unit};

fn optional(pointer: *const c_char) -> Option<&'static [u8]> {
    if pointer.is_null() {
        None
    } else {
        // SAFETY: the caller's C string contract; the borrow lives for the call.
        Some(unsafe { crate::ffi::cstr::slice(pointer) })
    }
}

/// Restrict the process promises.
///
/// # Safety
/// Each argument is NULL or a NUL-terminated promise string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn pledge(promises: *const c_char, execpromises: *const c_char) -> c_int {
    let promises = optional(promises);
    let exec = optional(execpromises);
    ret_unit(PLEDGE_STATE.with(|state| {
        raven_libc_personality_bsd::pledge_glue::pledge(&mut RealPledge, state, promises, exec)
    }))
}
