// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the getmntent_r export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `getmntent_r` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use super::mntent_state::{CMntEnt, store};
use raven_libc_compat::mntent::parse_line;

/// `getmntent_r`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getmntent_r(
    file: *mut super::File,
    entry: *mut CMntEnt,
    line_buffer: *mut c_char,
    line_length: c_int,
) -> *mut CMntEnt {
    if file.is_null() || entry.is_null() {
        return core::ptr::null_mut();
    }
    let mut storage = [0u8; 8192];
    let (buffer, length) = if line_buffer.is_null() || line_length <= 0 {
        (&mut storage[..], 8192usize)
    } else {
        // SAFETY: the caller gives a `line_length` buffer.
        (
            unsafe {
                core::slice::from_raw_parts_mut(
                    line_buffer.cast::<u8>(),
                    usize::try_from(line_length).unwrap_or(0),
                )
            },
            usize::try_from(line_length).unwrap_or(0),
        )
    };
    loop {
        // SAFETY: `file` is open and the buffer writable.
        let read = unsafe {
            super::fgets(
                buffer.as_mut_ptr().cast(),
                c_int::try_from(length).unwrap_or(8192),
                file,
            )
        };
        if read.is_null() {
            return core::ptr::null_mut();
        }
        // SAFETY: `fgets` NUL-terminated the buffer.
        let used = unsafe { super::c_len(buffer.as_ptr().cast(), length) };
        let line = &buffer[..used];
        if line.first().copied() == Some(b'#') || line.iter().all(u8::is_ascii_whitespace) {
            continue;
        }
        if let Some(parsed) = parse_line(line) {
            let stored = unsafe { store(&parsed) };
            // SAFETY: `entry` is writable; copy the parsed pointers.
            unsafe { core::ptr::copy_nonoverlapping(stored, entry, 1) };
            return entry;
        }
    }
}
