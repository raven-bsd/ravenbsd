// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mblen(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_ctype::MbMode;

/// The length of the next multi-byte character without a state.
///
/// # Safety
/// `s` has `n` readable bytes (or is NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mblen(s: *const c_char, n: usize) -> c_int {
    if s.is_null() {
        return 0;
    }
    // SAFETY: the caller grants `n` readable bytes.
    let input = unsafe { core::slice::from_raw_parts(s.cast::<u8>(), n) };
    match raven_libc_ctype::mblen(MbMode::Utf8, input) {
        Ok(length) => c_int::try_from(length).unwrap_or(-1),
        Err(_) => {
            crate::ffi::errno::set_raw(84);
            -1
        }
    }
}
