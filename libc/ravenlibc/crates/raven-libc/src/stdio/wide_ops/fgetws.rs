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

/// `fgetws`: read a wide line into `dst`.
///
/// # Safety
/// `dst` has `size` writable `wchar_t` units; `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fgetws(dst: *mut u32, size: c_int, stream: *mut c_void) -> *mut u32 {
    let Some(target) = file(stream) else {
        return core::ptr::null_mut();
    };
    if dst.is_null() || size <= 0 {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(22));
        return core::ptr::null_mut();
    }
    // SAFETY: the caller grants `size` writable wide units.
    let buffer = unsafe { core::slice::from_raw_parts_mut(dst, size as usize) };
    match raven_libc_stdio::fgetws(target, buffer) {
        Ok(_) => dst,
        Err(Fail::Errno(value)) => {
            crate::ffi::errno::set(value);
            core::ptr::null_mut()
        }
        Err(Fail::Eof) => core::ptr::null_mut(),
    }
}

/// Length of a NUL-terminated wide string, bounded by `limit`.
fn wide_length(value: *const u32, limit: usize) -> usize {
    let mut length = 0usize;
    while length < limit {
        // SAFETY: the caller guarantees `limit` readable units or a NUL.
        if unsafe { *value.add(length) } == 0 {
            break;
        }
        length += 1;
    }
    length
}

/// `fputws`: write a NUL-terminated wide string.
///
/// # Safety
/// `text` is NUL-terminated; `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fputws(text: *const u32, stream: *mut c_void) -> c_int {
    let Some(target) = file(stream) else {
        return -1;
    };
    if text.is_null() {
        crate::ffi::errno::set(raven_libc_abi::errno::Errno(22));
        return -1;
    }
    let length = wide_length(text, 1 << 20);
    // SAFETY: the caller's NUL-terminated string has `length` readable units.
    let value = unsafe { core::slice::from_raw_parts(text, length) };
    match raven_libc_stdio::fputws(target, value) {
        Ok(()) => 0,
        Err(Fail::Errno(error)) => {
            crate::ffi::errno::set(error);
            -1
        }
        Err(Fail::Eof) => -1,
    }
}

/// `fputws_unlocked`.
///
/// # Safety
/// As `fputws`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fputws_unlocked(text: *const u32, stream: *mut c_void) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { fputws(text, stream) }
}

/// `fputwc`: write one wide character.
///
/// # Safety
/// `stream` is a FILE.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fputwc(wc: u32, stream: *mut c_void) -> u32 {
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
