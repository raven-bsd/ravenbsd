// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `munlock`.

use super::*;

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
