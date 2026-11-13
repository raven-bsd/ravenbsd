// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the backtrace_symbols export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `backtrace_symbols` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use alloc::vec::Vec;
use raven_libc_compat::execinfo::format_symbol;

/// `backtrace_symbols`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn backtrace_symbols(
    buffer: *const *mut core::ffi::c_void,
    size: c_int,
) -> *mut *mut c_char {
    if buffer.is_null() || size <= 0 {
        return core::ptr::null_mut();
    }
    let count = usize::try_from(size).unwrap_or(0);
    let mut strings: Vec<Vec<u8>> = Vec::with_capacity(count);
    for index in 0..count {
        // SAFETY: the caller gives `size` readable pointers.
        let address = unsafe { *buffer.add(index) } as usize;
        let mut text = Vec::new();
        format_symbol(&mut text, b"", b"", 0, address);
        strings.push(text);
    }
    let pointers = count.saturating_mul(core::mem::size_of::<*mut c_char>());
    let bytes: usize = strings
        .iter()
        .map(|text| text.len().saturating_add(1))
        .sum();
    // SAFETY: one block holds the pointer array followed by the strings.
    let block = unsafe { super::malloc(pointers.saturating_add(bytes)) }.cast::<u8>();
    if block.is_null() {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::ENOMEM) };
        return core::ptr::null_mut();
    }
    // SAFETY: the block is `pointers + bytes` bytes.
    unsafe {
        let array = block.cast::<*mut c_char>();
        let mut cursor = block.add(pointers);
        for (index, text) in strings.iter().enumerate() {
            *array.add(index) = cursor.cast::<c_char>();
            core::ptr::copy_nonoverlapping(text.as_ptr(), cursor, text.len());
            *cursor.add(text.len()) = 0;
            cursor = cursor.add(text.len().saturating_add(1));
        }
        array
    }
}
