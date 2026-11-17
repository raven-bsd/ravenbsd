// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c exports, grouped so each codegen unit stays within the K4 C-symbol budget.

use super::*;

/// Create a pipe.
///
/// # Safety
/// `fds` points to two writable `int` values.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pipe(fds: *mut c_int) -> c_int {
    if fds.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match global().pipe() {
        Ok((read, write)) => {
            // SAFETY: the caller supplied two writable ints.
            unsafe {
                core::ptr::write(fds, read);
                core::ptr::write(fds.add(1), write);
            }
            0
        }
        Err(error) => ret::int(Err(map_error(error))),
    }
}
/// Close a descriptor with `posix_close` semantics.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn posix_close(fd: c_int, flags: c_int) -> c_int {
    if flags != 0 {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EINVAL as u16,
        )));
    }
    unit(raven_libc_platform::compat::posix_close(global(), fd))
}
/// Read a symbolic link relative to `dirfd`.
///
/// # Safety
/// `path` is NUL terminated; `buf` has `len` writable bytes.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn readlinkat(
    dirfd: c_int,
    path: *const c_char,
    buf: *mut c_char,
    len: usize,
) -> isize {
    if path.is_null() {
        return ret::ssize(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    // SAFETY: the caller's writable buffer contract.
    let buf = unsafe { crate::ffi::cstr::slice_mut(buf, len) };
    ssize(raven_libc_platform::compat::readlinkat(
        global(),
        dirfd,
        path,
        buf,
    ))
}
/// Set the host name.
///
/// # Safety
/// `name` has `len` readable bytes.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn sethostname(name: *const c_char, len: usize) -> c_int {
    // SAFETY: the caller's readable buffer contract.
    let name = unsafe { crate::ffi::cstr::bytes_n(name, len) };
    unit(global().sethostname(name))
}
