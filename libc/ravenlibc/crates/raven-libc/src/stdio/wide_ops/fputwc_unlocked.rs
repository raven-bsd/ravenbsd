// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::c_void;

use raven_libc_stdio::{Fail, file_from_ptr};

fn file(pointer: *mut c_void) -> Option<&'static raven_libc_stdio::File<crate::ffi::env::ShellEnv>> {
    if pointer.is_null() {
        None
    } else {
        Some(file_from_ptr(pointer.cast()))
    }
}

fn put(wc: u32, stream: *mut c_void) -> u32 {
    match file(stream) {
        Some(target) => match raven_libc_stdio::fputwc(target, wc) {
            Ok(value) => value,
            Err(Fail::Errno(error)) => {
                crate::ffi::errno::set(error);
                u32::MAX
            }
            Err(Fail::Eof) => u32::MAX,
        },
        None => u32::MAX,
    }
}

/// `fputwc_unlocked`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fputwc_unlocked(wc: u32, stream: *mut c_void) -> u32 {
    put(wc, stream)
}

/// `putwc`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putwc(wc: u32, stream: *mut c_void) -> u32 {
    put(wc, stream)
}

/// `putwc_unlocked`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn putwc_unlocked(wc: u32, stream: *mut c_void) -> u32 {
    put(wc, stream)
}

/// `putwchar`: one wide character to stdout.
#[unsafe(no_mangle)]
pub extern "C" fn putwchar(wc: u32) -> u32 {
    match raven_libc_stdio::putwchar(crate::stdio::streams::streams().state(), wc) {
        Ok(value) => value,
        Err(Fail::Errno(error)) => {
            crate::ffi::errno::set(error);
            u32::MAX
        }
        Err(Fail::Eof) => u32::MAX,
    }
}
