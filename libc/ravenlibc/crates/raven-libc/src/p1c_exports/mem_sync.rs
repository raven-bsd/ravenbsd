// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c exports, grouped so each codegen unit stays within the K4 C-symbol budget.

use super::*;

/// Synchronize a mapping with its backing store.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn msync(addr: *mut c_void, len: usize, flags: c_int) -> c_int {
    unit(raven_libc_platform::mem::msync(
        global(),
        addr as usize,
        len,
        flags,
    ))
}
/// Unlock all mapped pages.
///
/// # Safety
/// C ABI no arguments.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn munlockall() -> c_int {
    unit(raven_libc_platform::mem::munlockall(global()))
}
/// Unlock a memory range.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn munlock(addr: *const c_void, len: usize) -> c_int {
    unit(raven_libc_platform::mem::munlock(
        global(),
        addr as usize,
        len,
    ))
}
/// POSIX memory advice.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn posix_madvise(addr: *mut c_void, len: usize, advice: c_int) -> c_int {
    unit(raven_libc_platform::compat::posix_madvise(
        global(),
        addr as usize,
        len,
        advice,
    ))
}
