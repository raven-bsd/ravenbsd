// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the getbsize export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `getbsize` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

static mut HEADER: [u8; 20] = [0; 20];

/// `getbsize`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getbsize(headerlen: *mut c_int, blocksize: *mut i64) -> *mut c_char {
    if headerlen.is_null() || blocksize.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: `getenv` is the libc accessor; the returned string is NUL-terminated.
    let blocksize_name = b"BLOCKSIZE\0".as_ptr().cast::<c_char>();
    let value = unsafe { super::getenv(blocksize_name) };
    let env = if value.is_null() {
        None
    } else {
        // SAFETY: the environment value is NUL-terminated.
        Some(unsafe { super::c_bytes(value, 256) })
    };
    let mut length = 0i32;
    let mut size = 0i64;
    let header = raven_libc_compat::getbsize::getbsize(env, &mut length, &mut size);
    // SAFETY: `HEADER` is a private static and `header` is at most 19 bytes.
    unsafe {
        let storage = &mut *(&raw mut HEADER);
        for (slot, byte) in storage.iter_mut().zip(header.iter()) {
            *slot = *byte;
        }
        for slot in storage.iter_mut().skip(header.len()) {
            *slot = 0;
        }
        *headerlen = length;
        *blocksize = size;
        storage.as_mut_ptr().cast::<c_char>()
    }
}
