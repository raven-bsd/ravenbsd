// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c exports, grouped so each codegen unit stays within the K4 C-symbol budget.

use super::*;

/// Advise the kernel about a file range.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn posix_fadvise(fd: c_int, offset: i64, len: i64, advice: c_int) -> c_int {
    unit(global().posix_fadvise(fd, offset, len, advice))
}
/// Allocate file space.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn posix_fallocate(fd: c_int, offset: i64, len: i64) -> c_int {
    unit(global().posix_fallocate(fd, offset, len))
}
/// Change a symlink's mode (unsupported on Raven).
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn lchmod(path: *const c_char, mode: u32) -> c_int {
    if path.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    unit(raven_libc_platform::compat::lchmod(global(), path, mode))
}
/// Lock a file range.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn lockf(fd: c_int, function: c_int, size: i64) -> c_int {
    unit(global().lockf(fd, function, size))
}
