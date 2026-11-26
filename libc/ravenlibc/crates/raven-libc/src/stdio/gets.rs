// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `gets`, `renameat2` and `vsnprintf_l`.

use core::ffi::{c_char, c_int};
use core::sync::atomic::Ordering;

use crate::stdio::va::List;

/// `gets`: read a line from stdin (deprecated and unbounded, as specified).
///
/// # Safety
/// `dst` is writable for the line plus NUL.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gets(dst: *mut c_char) -> *mut c_char {
    if dst.is_null() {
        return core::ptr::null_mut();
    }
    let mut index = 0usize;
    loop {
        let stream = crate::stdio::stdin::stdin.load(Ordering::Acquire);
        // SAFETY: the standard input FILE is published by the start path.
        let ch = unsafe { crate::stdio::file::fgetc_impl(stream) };
        if ch == -1 {
            if index == 0 {
                return core::ptr::null_mut();
            }
            break;
        }
        if ch == i32::from(b'\n') {
            break;
        }
        // SAFETY: the caller grants the unbounded C buffer.
        unsafe { *dst.add(index) = ch as c_char };
        index += 1;
    }
    // SAFETY: the caller grants the terminating byte.
    unsafe { *dst.add(index) = 0 };
    dst
}

/// `renameat2`: rename with flags (only the plain rename when `flags == 0`).
///
/// # Safety
/// The path pointers are NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn renameat2(
    _olddirfd: c_int,
    oldpath: *const c_char,
    _newdirfd: c_int,
    newpath: *const c_char,
    flags: u32,
) -> c_int {
    if flags != 0 {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno::from_generated(
            raven_libc_abi::errno::ENOSYS,
        )));
    }
    if oldpath.is_null() || newpath.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno::from_generated(
            raven_libc_abi::errno::EINVAL,
        )));
    }
    // SAFETY: both paths are NUL-terminated.
    let old = unsafe { crate::ffi::cstr::slice(oldpath) };
    // SAFETY: both paths are NUL-terminated.
    let new = unsafe { crate::ffi::cstr::slice(newpath) };
    match crate::ffi::platform::global().rename(old, new) {
        Ok(()) => 0,
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            -1
        }
    }
}

/// `vsnprintf_l`: `vsnprintf` with a locale object (the object is accepted and ignored).
///
/// # Safety
/// As `vsnprintf`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vsnprintf_l(
    buffer: *mut c_char,
    cap: usize,
    format: *const c_char,
    list: *mut List,
    _locale: *mut core::ffi::c_void,
) -> c_int {
    // SAFETY: forwarded C contract.
    unsafe { crate::stdio::vsnprintf::vsnprintf(buffer, cap, format, list) }
}
