// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `preadv`.

use super::*;

/// Vector read at an explicit offset.
///
/// # Safety
/// `iovs` has `count` readable iovec records.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn preadv(fd: c_int, iovs: *const Iovec, count: c_int, offset: i64) -> isize {
    // SAFETY: the caller supplied `count` readable iovec records.
    let mut bufs = unsafe { iovec_mut(iovs, count) };
    ssize(raven_libc_platform::compat::preadv(
        global(),
        fd,
        &mut bufs,
        offset,
    ))
}
