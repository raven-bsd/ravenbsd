// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `setspent`, `endspent` and `fgetspent`.

use core::ffi::c_void;

use super::shadow;

/// `setspent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn setspent() {
    shadow::end();
    shadow::rewind();
}

/// `endspent(3)`.
#[unsafe(no_mangle)]
pub extern "C" fn endspent() {
    shadow::end();
}

/// `fgetspent(3)`.
///
/// # Safety
/// `stream` is a FILE opened by `fopen`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetspent(stream: *mut c_void) -> *mut shadow::Spwd {
    if stream.is_null() {
        return core::ptr::null_mut();
    }
    // SAFETY: the caller's FILE pointer.
    let file: &raven_libc_stdio::File<crate::ffi::env::ShellEnv> =
        raven_libc_stdio::file_from_ptr(stream.cast());
    let mut line = [0u8; 1024];
    loop {
        let length = match raven_libc_stdio::fgets(file, &mut line) {
            Ok(length) => length,
            Err(_) => return core::ptr::null_mut(),
        };
        if length == 0 {
            return core::ptr::null_mut();
        }
        let text = match line.get(..length) {
            Some(text) => text,
            None => continue,
        };
        let text = text.strip_suffix(b"\n").unwrap_or(text);
        let text = text.strip_suffix(b"\r").unwrap_or(text);
        if let Some(entry) = raven_libc_pwd::parse_shadow_line(text) {
            return shadow::fill_static(&entry);
        }
    }
}
