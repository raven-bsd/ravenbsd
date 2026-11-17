// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_int, c_void};

use raven_libc_stdio::{Fail, file_from_ptr};

fn file(pointer: *mut c_void) -> Option<&'static raven_libc_stdio::File<crate::ffi::env::ShellEnv>> {
    if pointer.is_null() {
        None
    } else {
        Some(file_from_ptr(pointer.cast()))
    }
}

fn fail(error: Fail) -> u32 {
    if let Fail::Errno(value) = error {
        crate::ffi::errno::set(value);
    }
    u32::MAX
}

/// `getwc_unlocked`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getwc_unlocked(stream: *mut c_void) -> u32 {
    match file(stream) {
        Some(target) => match raven_libc_stdio::getwc(target) {
            Ok(value) => value,
            Err(error) => fail(error),
        },
        None => u32::MAX,
    }
}

/// `fgetwc_unlocked`.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetwc_unlocked(stream: *mut c_void) -> u32 {
    // SAFETY: forwarded C contract.
    unsafe { getwc_unlocked(stream) }
}

/// `ungetwc`: push one wide character back.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ungetwc(wc: u32, stream: *mut c_void) -> u32 {
    match file(stream) {
        Some(target) => match raven_libc_stdio::ungetwc(target, wc) {
            Ok(value) => value,
            Err(error) => fail(error),
        },
        None => u32::MAX,
    }
}
