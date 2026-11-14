// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `inet_ntop(3)`.
use core::ffi::{c_char, c_int, c_void};

use raven_libc_abi::socket::{AF_INET, AF_INET6};

/// Format an address into the caller's buffer.
///
/// # Safety
/// `src` has `af`'s byte count; `dst` has `size` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn inet_ntop(
    af: c_int,
    src: *const c_void,
    dst: *mut c_char,
    size: u32,
) -> *mut c_char {
    if src.is_null() || dst.is_null() {
        return crate::ffi::ret::null(crate::process::exec_common::fault());
    }
    let source_size = match af {
        AF_INET => 4usize,
        AF_INET6 => 16usize,
        _ => return crate::ffi::map_net::ntop(af, &[], &mut []),
    };
    // SAFETY: the caller's source and destination buffers.
    let (source, out) = unsafe {
        (
            core::slice::from_raw_parts(src.cast::<u8>(), source_size),
            core::slice::from_raw_parts_mut(dst.cast::<u8>(), size as usize),
        )
    };
    crate::ffi::map_net::ntop(af, source, out)
}
