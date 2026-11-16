// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `mbstowcs(3)`.
//!
//! `raven-libc-ctype` owns the UTF-8 conversion; this file owns the C string, destination
//! capacity and errno contract.

use core::ffi::{c_char, c_int};

use raven_libc_ctype::MbMode;

/// `mbstowcs`: convert a NUL-terminated multibyte string.
///
/// # Safety
/// `src` is NUL-terminated; `dst` is NULL or writable for `n` `wchar_t` elements.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mbstowcs(dst: *mut c_int, src: *const c_char, n: usize) -> usize {
    // SAFETY: the caller's `strlen` contract.
    let length = unsafe { raven_libc_str::raw::strlen(src.cast()) };
    // SAFETY: the caller guarantees the NUL at `length`; include it for the engine's terminator.
    let input = unsafe { core::slice::from_raw_parts(src.cast::<u8>(), length.saturating_add(1)) };
    let output = if dst.is_null() {
        None
    } else {
        // SAFETY: the caller grants `n` writable `wchar_t` elements.
        Some(unsafe { core::slice::from_raw_parts_mut(dst.cast::<u32>(), n) })
    };
    match raven_libc_ctype::mbstowcs(MbMode::Utf8, input, output).ret {
        Ok(count) => count,
        Err(_) => {
            crate::ffi::errno::set_raw(c_int::from(raven_libc_abi::errno::EILSEQ));
            usize::MAX
        }
    }
}
