// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `initstate(3)`.
use core::ffi::{c_char, c_int};

/// Switch the process generator to the caller's state buffer.
///
/// # Safety
/// `state` is NULL or points to `bytes` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn initstate(seed: u32, state: *mut c_char, bytes: usize) -> *mut c_char {
    // SAFETY: the caller's state buffer contract.
    match unsafe { crate::stdlib::rand_state::init(seed, state, bytes) } {
        Ok(previous) => previous,
        Err(error) => crate::ffi::ret::null::<c_int>(error).cast::<c_char>(),
    }
}
