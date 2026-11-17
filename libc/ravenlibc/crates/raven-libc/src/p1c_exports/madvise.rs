// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `madvise`.

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
