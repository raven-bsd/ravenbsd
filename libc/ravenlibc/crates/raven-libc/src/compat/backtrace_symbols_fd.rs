// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the backtrace_symbols_fd export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `backtrace_symbols_fd` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use alloc::vec::Vec;
use raven_libc_compat::execinfo::format_symbol;

/// `backtrace_symbols_fd`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn backtrace_symbols_fd(
    buffer: *const *mut core::ffi::c_void,
    size: c_int,
    fd: c_int,
) {
    if buffer.is_null() || size <= 0 {
        return;
    }
    let count = usize::try_from(size).unwrap_or(0);
    for index in 0..count {
        // SAFETY: the caller gives `size` readable pointers.
        let address = unsafe { *buffer.add(index) } as usize;
        let mut text: Vec<u8> = Vec::new();
        format_symbol(&mut text, b"", b"", 0, address);
        text.push(b'\n');
        // SAFETY: `text` is readable; `fd` is the caller's descriptor.
        unsafe { super::write(fd, text.as_ptr().cast(), text.len()) };
    }
}
