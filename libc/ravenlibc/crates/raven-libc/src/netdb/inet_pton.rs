// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inet_pton(3)`.
use core::ffi::{c_char, c_int, c_void};

use raven_libc_abi::socket::{AF_INET, AF_INET6};

/// Parse a text address into the caller's 4- or 16-byte buffer.
///
/// # Safety
/// `text` is NUL-terminated; `dst` has enough bytes for `af`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inet_pton(af: c_int, text: *const c_char, dst: *mut c_void) -> c_int {
    if text.is_null() || dst.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    let size = match af {
        AF_INET => 4usize,
        AF_INET6 => 16usize,
        _ => return crate::ffi::map_net::pton(af, &[], &mut []),
    };
    // SAFETY: the caller's string and `size`-byte destination.
    let (text, out) = unsafe {
        (
            crate::ffi::cstr::slice(text),
            core::slice::from_raw_parts_mut(dst.cast::<u8>(), size),
        )
    };
    crate::ffi::map_net::pton(af, text, out)
}
