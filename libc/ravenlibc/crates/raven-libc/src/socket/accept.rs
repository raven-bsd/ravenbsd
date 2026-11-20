// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `accept`, `accept4`, `bind` and `connect`.

use core::ffi::{c_int, c_void};

use super::*;

fn accept_common(fd: c_int, addr: *mut c_void, len: *mut SockLen, flags: c_int) -> c_int {
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
    let _ = raven_libc_sys::sys::debug_write(b"ka:begin\n");
    match with_net(|net| net.accept(fd, out, flags)) {
        Ok((new_fd, full)) => {
            // SAFETY: the caller's in/out socklen.
            unsafe { crate::ffi::sockaddr::write_len(len, full, capacity) };
            let _ = raven_libc_sys::sys::debug_write(b"ka:ok\n");
            new_fd
        }
        Err(error) => {
            let _ =
                raven_libc_sys::sys::debug_write(alloc::format!("ka:err {}\n", error.0).as_bytes());
            crate::ffi::ret::int(Err(error))
        }
    }
}

/// `accept(2)`.
///
/// # Safety
/// `addr`/`len` follow the in/out sockaddr contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn accept(fd: c_int, addr: *mut c_void, len: *mut SockLen) -> c_int {
    accept_common(fd, addr, len, 0)
}

/// `accept4(2)`.
///
/// # Safety
/// As [`accept`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn accept4(
    fd: c_int,
    addr: *mut c_void,
    len: *mut SockLen,
    flags: c_int,
) -> c_int {
    accept_common(fd, addr, len, flags)
}

/// `bind(2)`.
///
/// # Safety
/// `addr` points to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bind(fd: c_int, addr: *const c_void, len: SockLen) -> c_int {
    // SAFETY: the caller's sockaddr contract.
    let bytes = match unsafe { crate::ffi::sockaddr::view(addr, len) } {
        Ok(bytes) => bytes,
        Err(error) => return crate::ffi::ret::int(Err(error)),
    };
    unit_result(with_net(|net| net.bind(fd, bytes)))
}

/// `connect(2)`.
///
/// # Safety
/// `addr` points to `len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn connect(fd: c_int, addr: *const c_void, len: SockLen) -> c_int {
    // SAFETY: the caller's sockaddr contract.
    let bytes = match unsafe { crate::ffi::sockaddr::view(addr, len) } {
        Ok(bytes) => bytes,
        Err(error) => return crate::ffi::ret::int(Err(error)),
    };
    unit_result(with_net(|net| net.connect(fd, bytes)))
}
