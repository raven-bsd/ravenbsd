// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `preadv2`.

use super::*;

/// Vector read with flags.
///
/// # Safety
/// `iovs` has `count` readable iovec records.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn preadv2(
    fd: c_int,
    iovs: *const Iovec,
    count: c_int,
    offset: i64,
    flags: c_int,
) -> isize {
    // SAFETY: the caller supplied `count` readable iovec records.
    let mut bufs = unsafe { iovec_mut(iovs, count) };
    ssize(raven_libc_platform::compat::preadv2(
        global(),
        fd,
        &mut bufs,
        offset,
        flags,
    ))
}
