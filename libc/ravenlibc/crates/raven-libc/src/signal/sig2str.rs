// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sig2str(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_signal::sig2str::{SIG2STR_MAX, sig2str as sig2str_core};

/// Write the name of `sig` into `buf`.
///
/// # Safety
/// `buf` is writable for at least `SIG2STR_MAX` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sig2str(sig: c_int, buf: *mut c_char) -> c_int {
    if buf.is_null() {
        return -1;
    }
    let mut out = [0u8; SIG2STR_MAX];
    match sig2str_core(sig, &mut out) {
        Ok(length) => {
            // SAFETY: the caller provides SIG2STR_MAX writable bytes.
            unsafe {
                core::ptr::copy_nonoverlapping(out.as_ptr(), buf.cast(), SIG2STR_MAX);
            }
            let _ = length;
            0
        }
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}
