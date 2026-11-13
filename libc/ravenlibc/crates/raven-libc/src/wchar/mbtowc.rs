// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mbtowc(3)` and `wctomb(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_ctype::MbMode;

/// Decode one wide character without a conversion state.
///
/// # Safety
/// `s` has `n` readable bytes (or is NULL); `pwc` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbtowc(pwc: *mut c_int, s: *const c_char, n: usize) -> c_int {
    if s.is_null() {
        return 0;
    }
    // SAFETY: the caller grants `n` readable bytes.
    let input = unsafe { core::slice::from_raw_parts(s.cast::<u8>(), n) };
    match raven_libc_ctype::mbtowc(MbMode::Utf8, input) {
        Ok((wc, used)) => {
            if !pwc.is_null() {
                // SAFETY: `pwc` is a writable `wchar_t` slot by the caller's contract.
                unsafe { *pwc = wc as c_int };
            }
            c_int::try_from(used).unwrap_or(-1)
        }
        Err(_) => {
            crate::ffi::errno::set_raw(84);
            -1
        }
    }
}

/// Encode `wc` without a conversion state.
///
/// # Safety
/// `s` has four writable bytes (or is NULL).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wctomb(s: *mut c_char, wc: u32) -> c_int {
    if s.is_null() {
        return 0;
    }
    let mut buffer = [0u8; 4];
    match raven_libc_ctype::wctomb(MbMode::Utf8, wc, &mut buffer) {
        Ok(length) => {
            let bytes = buffer.get(..length).unwrap_or(&buffer);
            // SAFETY: the caller grants four writable bytes; `bytes.len() <= 4`.
            unsafe { crate::ffi::out::write_bytes(s.cast(), bytes, 4) };
            c_int::try_from(length).unwrap_or(-1)
        }
        Err(_) => {
            crate::ffi::errno::set_raw(84);
            -1
        }
    }
}
