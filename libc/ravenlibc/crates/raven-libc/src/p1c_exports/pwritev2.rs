// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `pwritev2`.

use super::*;

/// Vector write with flags.
///
/// # Safety
/// `iovs` has `count` readable iovec records.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pwritev2(
    fd: c_int,
    iovs: *const Iovec,
    count: c_int,
    offset: i64,
    flags: c_int,
) -> isize {
    // SAFETY: the caller supplied `count` readable iovec records.
    let bufs = unsafe { iovec_ref(iovs, count) };
    ssize(raven_libc_platform::compat::pwritev2(
        global(),
        fd,
        &bufs,
        offset,
        flags,
    ))
}
