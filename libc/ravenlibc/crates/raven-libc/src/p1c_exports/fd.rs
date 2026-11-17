// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c exports, grouped so each codegen unit stays within the K4 C-symbol budget.

use super::*;

/// Truncate a descriptor to a 64-bit size.
///
/// # Safety
/// C ABI scalar arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn ftruncate64(fd: c_int, size: i64) -> c_int {
    unit(raven_libc_platform::compat::ftruncate64(global(), fd, size))
}
/// Read packed directory entries.
///
/// # Safety
/// `dirp` has `count` writable bytes.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getdents(fd: c_int, dirp: *mut c_void, count: usize) -> c_int {
    // SAFETY: the caller's buffer contract; NULL with zero is the empty slice.
    let buffer = unsafe { crate::ffi::cstr::slice_mut(dirp.cast(), count) };
    int(raven_libc_platform::compat::getdents(global(), fd, buffer)
        .map(|written| c_int::try_from(written).unwrap_or(c_int::MAX)))
}
/// Read a 64-bit resource limit.
///
/// # Safety
/// `limit` points to a writable `struct rlimit`.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn getrlimit64(resource: c_int, limit: *mut Rlimit) -> c_int {
    if limit.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match raven_libc_platform::compat::getrlimit64(global(), resource) {
        Ok((soft, hard)) => {
            // SAFETY: the caller supplied a writable Rlimit.
            unsafe {
                *limit = Rlimit {
                    rlim_cur: soft,
                    rlim_max: hard,
                }
            };
            0
        }
        Err(error) => ret::int(Err(map_error(error))),
    }
}
/// Read a path configuration value.
///
/// # Safety
/// `path` is NUL terminated.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pathconf(path: *const c_char, name: c_int) -> c_long {
    if path.is_null() {
        return ret::long(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    match global().pathconf(path, name) {
        Ok(value) => c_long::try_from(value).unwrap_or(c_long::MAX),
        Err(error) => ret::long(Err(map_error(error))),
    }
}
