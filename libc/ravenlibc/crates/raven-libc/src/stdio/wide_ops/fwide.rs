// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use core::ffi::{c_int, c_void};

use raven_libc_stdio::{Fail, file_from_ptr};

fn file(
    pointer: *mut c_void,
) -> Option<&'static raven_libc_stdio::File<crate::ffi::env::ShellEnv>> {
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

/// `fwide`: query or set the stream orientation.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fwide(stream: *mut c_void, mode: c_int) -> c_int {
    match file(stream) {
        Some(target) => raven_libc_stdio::fwide(target, mode),
        None => {
            crate::ffi::errno::set(raven_libc_abi::errno::Errno(9));
            0
        }
    }
}

/// `getwchar`: one wide character from stdin.
#[unsafe(no_mangle)]
pub extern "C" fn getwchar() -> u32 {
    match raven_libc_stdio::getwchar(crate::stdio::streams::streams().state()) {
        Ok(value) => value,
        Err(error) => fail(error),
    }
}

/// `getwchar_unlocked`.
#[unsafe(no_mangle)]
pub extern "C" fn getwchar_unlocked() -> u32 {
    getwchar()
}

/// `getwc`: one wide character from a FILE.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getwc(stream: *mut c_void) -> u32 {
    match file(stream) {
        Some(target) => match raven_libc_stdio::getwc(target) {
            Ok(value) => value,
            Err(error) => fail(error),
        },
        None => u32::MAX,
    }
}
