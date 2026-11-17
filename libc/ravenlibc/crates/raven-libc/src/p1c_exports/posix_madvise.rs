// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `posix_madvise`.

use super::*;

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
