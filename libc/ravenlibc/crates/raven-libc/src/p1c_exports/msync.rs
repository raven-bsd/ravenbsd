// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `msync`.

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
