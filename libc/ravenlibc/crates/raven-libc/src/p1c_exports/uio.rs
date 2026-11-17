// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! P1c exports, grouped so each codegen unit stays within the K4 C-symbol budget.

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
/// Vector write at an explicit offset.
///
/// # Safety
/// `iovs` has `count` readable iovec records.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn pwritev(
    fd: c_int,
    iovs: *const Iovec,
    count: c_int,
    offset: i64,
) -> isize {
    // SAFETY: the caller supplied `count` readable iovec records.
    let bufs = unsafe { iovec_ref(iovs, count) };
    ssize(raven_libc_platform::compat::pwritev(
        global(),
        fd,
        &bufs,
        offset,
    ))
}
