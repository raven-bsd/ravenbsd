// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `brk`.

use super::*;

/// Set the program break (class B: ENOSYS).
///
/// # Safety
/// C ABI scalar argument.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn brk(addr: *mut c_void) -> c_int {
    unit(raven_libc_platform::mem::brk(global(), addr as usize).map(|_| ()))
}
