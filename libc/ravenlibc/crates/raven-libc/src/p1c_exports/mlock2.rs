// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `mlock2`.

use super::*;

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
