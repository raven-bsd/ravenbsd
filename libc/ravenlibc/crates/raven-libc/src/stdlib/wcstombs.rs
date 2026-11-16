// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wcstombs(3)`.
//!
//! `raven-libc-ctype` owns the UTF-8 conversion; this file owns the C wide-string, destination
//! capacity and errno contract.

use core::ffi::{c_char, c_int};

use raven_libc_ctype::MbMode;

/// `wcstombs`: convert a NUL-terminated wide string.
///
/// # Safety
/// `src` is NUL-terminated; `dst` is NULL or writable for `n` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstombs(dst: *mut c_char, src: *const c_int, n: usize) -> usize {
    let mut length = 0usize;
    loop {
        // SAFETY: the caller guarantees a NUL-terminated wide string.
        if unsafe { *src.add(length) } == 0 {
            break;
        }
        length = length.saturating_add(1);
    }
    // SAFETY: include the wide NUL for the engine's terminator.
    let input = unsafe { core::slice::from_raw_parts(src.cast::<u32>(), length.saturating_add(1)) };
    let output = if dst.is_null() {
        None
    } else {
        // SAFETY: the caller grants `n` writable bytes.
        Some(unsafe { core::slice::from_raw_parts_mut(dst.cast::<u8>(), n) })
    };
    match raven_libc_ctype::wcstombs(MbMode::Utf8, input, output).ret {
        Ok(count) => count,
        Err(_) => {
            crate::ffi::errno::set_raw(c_int::from(raven_libc_abi::errno::EILSEQ));
            usize::MAX
        }
    }
}
