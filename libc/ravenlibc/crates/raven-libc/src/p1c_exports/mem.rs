// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c exports, grouped so each codegen unit stays within the K4 C-symbol budget.

use super::*;

/// Give advice about a memory range.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn madvise(addr: *mut c_void, len: usize, advice: c_int) -> c_int {
    unit(raven_libc_platform::mem::madvise(
        global(),
        addr as usize,
        len,
        advice,
    ))
}
/// Lock a memory range with flags.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn mlock2(addr: *const c_void, len: usize, flags: c_uint) -> c_int {
    unit(raven_libc_platform::compat::mlock2(
        global(),
        addr as usize,
        len,
        flags as i32,
    ))
}
/// Lock a memory range.
///
/// # Safety
/// `addr`/`len` name the caller's mapping.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn mlock(addr: *const c_void, len: usize) -> c_int {
    unit(raven_libc_platform::mem::mlock(
        global(),
        addr as usize,
        len,
    ))
}
/// Lock all mapped pages.
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn mlockall(flags: c_int) -> c_int {
    unit(raven_libc_platform::mem::mlockall(global(), flags))
}
