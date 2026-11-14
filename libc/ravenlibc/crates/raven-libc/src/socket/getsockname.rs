// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getsockname`, `getpeername`, `getsockopt` and `setsockopt`.

use core::ffi::{c_int, c_void};

use super::*;

fn name_result(result: Result<usize, Errno>, len: *mut SockLen, capacity: SockLen) -> c_int {
    match result {
        Ok(full) => {
            // SAFETY: the caller's in/out socklen.
            unsafe { crate::ffi::sockaddr::write_len(len, full, capacity) };
            0
        }
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}

/// `getsockname(2)`.
///
/// # Safety
/// `addr`/`len` follow the in/out sockaddr contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getsockname(fd: c_int, addr: *mut c_void, len: *mut SockLen) -> c_int {
    let capacity = if len.is_null() {
        0
    } else {
        // SAFETY: the caller's in/out socklen.
        unsafe { *len }
    };
    // SAFETY: the caller's writable sockaddr and its capacity.
    let out = match unsafe { crate::ffi::sockaddr::view_mut(addr, capacity) } {
        Ok(out) => out,
        Err(error) => return crate::ffi::ret::int(Err(error)),
    };
    let result = with_net(|net| net.getsockname(fd, out));
    name_result(result, len, capacity)
}

/// `getpeername(2)`.
///
/// # Safety
/// `addr`/`len` follow the in/out sockaddr contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpeername(fd: c_int, addr: *mut c_void, len: *mut SockLen) -> c_int {
    let capacity = if len.is_null() {
        0
    } else {
        // SAFETY: the caller's in/out socklen.
        unsafe { *len }
    };
    // SAFETY: the caller's writable sockaddr and its capacity.
    let out = match unsafe { crate::ffi::sockaddr::view_mut(addr, capacity) } {
        Ok(out) => out,
        Err(error) => return crate::ffi::ret::int(Err(error)),
    };
    let result = with_net(|net| net.getpeername(fd, out));
    name_result(result, len, capacity)
}

/// `getsockopt(2)`.
///
/// # Safety
/// `value` points to `*len` writable bytes; `len` is an in/out socklen.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getsockopt(
    fd: c_int,
    level: c_int,
    name: c_int,
    value: *mut c_void,
    len: *mut SockLen,
) -> c_int {
    let capacity = if len.is_null() {
        0
    } else {
        // SAFETY: the caller's in/out socklen.
        unsafe { *len }
    };
    // SAFETY: the caller's writable option buffer.
    let out = match unsafe { crate::ffi::sockaddr::view_mut(value, capacity) } {
        Ok(out) => out,
        Err(error) => return crate::ffi::ret::int(Err(error)),
    };
    match with_net(|net| net.getsockopt(fd, level, name, out)) {
        Ok(full) => {
            // SAFETY: the caller's in/out socklen.
            unsafe { crate::ffi::sockaddr::write_len(len, full, capacity) };
            0
        }
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}

/// `setsockopt(2)`.
///
/// # Safety
/// `value` points to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn setsockopt(
    fd: c_int,
    level: c_int,
    name: c_int,
    value: *const c_void,
    len: SockLen,
) -> c_int {
    // SAFETY: the caller's option buffer.
    let bytes = match unsafe { crate::ffi::sockaddr::view(value, len) } {
        Ok(bytes) => bytes,
        Err(error) => return crate::ffi::ret::int(Err(error)),
    };
    unit_result(with_net(|net| net.setsockopt(fd, level, name, bytes)))
}
