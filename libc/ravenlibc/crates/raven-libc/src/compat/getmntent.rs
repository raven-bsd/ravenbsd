// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the getmntent export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `getmntent` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::mntent_state::{CMntEnt, LINE, store};
use raven_libc_compat::mntent::parse_line;

/// `getmntent`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getmntent(file: *mut super::File) -> *mut CMntEnt {
    if file.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: `LINE` is a private static buffer.
    let buffer = unsafe { &mut *(&raw mut LINE) };
    let mut index = 0usize;
    loop {
        // SAFETY: `file` is open and the buffer writable.
        let read = unsafe {
            super::fgets(
                buffer.as_mut_ptr().cast(),
                c_int::try_from(buffer.len()).unwrap_or(8192),
                file,
            )
        };
        if read.is_null() {
            return core::ptr::null_mut();
        }
        // SAFETY: `fgets` NUL-terminated the buffer.
        let length = unsafe { super::c_len(buffer.as_ptr().cast(), buffer.len()) };
        let line = &buffer[..length];
        if index > 0
            && line.first().copied() != Some(b'#')
            && !line.iter().all(u8::is_ascii_whitespace)
        {
            if let Some(entry) = parse_line(line) {
                return unsafe { store(&entry) };
            }
        }
        index = index.wrapping_add(1);
        if index > 100_000 {
            return core::ptr::null_mut();
        }
    }
}
