// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `confstr(3)`.

use core::ffi::{c_char, c_int};

/// Copy the configuration value into `buf` and return its required size including the NUL.
///
/// # Safety
/// `buf` is NULL or `len` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn confstr(name: c_int, buf: *mut c_char, len: usize) -> usize {
    let out = if buf.is_null() || len == 0 {
        None
    } else {
        // SAFETY: the caller supplies `len` writable bytes.
        Some(unsafe { core::slice::from_raw_parts_mut(buf.cast::<u8>(), len) })
    };
    match raven_libc_platform::conf::confstr(name, out) {
        Ok(needed) => needed,
        Err(error) => {
            crate::ffi::errno::set(crate::platform::map_error(error));
            0
        }
    }
}
