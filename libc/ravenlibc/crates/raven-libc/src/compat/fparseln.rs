// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: C-ABI shell for the fparseln export (ticket C65, `17` section 13). The logic is in
// `raven-libc-compat`; this file owns the C struct, the caller buffers and the errno write.
//! `fparseln` C-ABI export (ticket C65).

use core::ffi::{c_char, c_int};

use alloc::vec::Vec;
use raven_libc_compat::LineSource;
use raven_libc_compat::fparseln as logic;

/// The `fgetln` line seam over a C `FILE`.
struct FileLines {
    file: *mut super::File,
}

impl LineSource for FileLines {
    fn next_line(&mut self) -> Option<Vec<u8>> {
        let mut length = 0usize;
        // SAFETY: `file` is the caller's open stream and `length` is writable.
        let line = unsafe { super::fgetln(self.file, &mut length) };
        if line.is_null() {
            return None;
        }
        // SAFETY: `fgetln` returned `length` readable bytes.
        Some(unsafe { core::slice::from_raw_parts(line.cast::<u8>(), length) }.to_vec())
    }
}

/// `fparseln`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fparseln(
    file: *mut super::File,
    size: *mut usize,
    lineno: *mut usize,
    delimiter: *const c_char,
    flags: c_int,
) -> *mut c_char {
    if file.is_null() {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::EINVAL) };
        return core::ptr::null_mut();
    }
    let mut delim = [b'\\', b'\\', b'#'];
    if !delimiter.is_null() {
        // SAFETY: the caller gives three delimiter characters.
        let bytes = unsafe { core::slice::from_raw_parts(delimiter.cast::<u8>(), 3) };
        delim = [bytes[0], bytes[1], bytes[2]];
    }
    let mut source = FileLines { file };
    let mut line_number = if lineno.is_null() {
        None
    } else {
        // SAFETY: `lineno` is writable.
        Some(unsafe { *lineno })
    };
    let line = logic::fparseln(&mut source, delim, flags as u32, line_number.as_mut());
    if !lineno.is_null() {
        if let Some(value) = line_number {
            // SAFETY: `lineno` is writable.
            unsafe { *lineno = value };
        }
    }
    let Some(line) = line else {
        return core::ptr::null_mut();
    };
    let pointer = unsafe { super::alloc_cstr(&line) };
    if pointer.is_null() {
        // SAFETY: the errno cell is this thread's.
        unsafe { super::set_errno(super::ENOMEM) };
        return pointer;
    }
    if !size.is_null() {
        // SAFETY: `size` is writable.
        unsafe { *size = line.len() };
    }
    pointer
}
