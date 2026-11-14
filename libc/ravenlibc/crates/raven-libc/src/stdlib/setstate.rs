// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setstate(3)`.
use core::ffi::{c_char, c_int};

/// Restore a state buffer produced by `initstate`/`setstate`.
///
/// # Safety
/// `state` is NULL or points to a state buffer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setstate(state: *mut c_char) -> *mut c_char {
    // SAFETY: the caller's state buffer contract.
    match unsafe { crate::stdlib::rand_state::restore(state) } {
        Ok(previous) => previous,
        Err(error) => crate::ffi::ret::null::<c_int>(error).cast::<c_char>(),
    }
}
