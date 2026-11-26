// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getpeereid(3)` for connected `AF_UNIX` endpoints.

use core::ffi::c_int;

use super::*;

/// `getpeereid(fd, uid, gid)`.
///
/// # Safety
/// `uid` and `gid` point to writable `uid_t`/`gid_t` values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpeereid(fd: c_int, uid: *mut u32, gid: *mut u32) -> c_int {
    if uid.is_null() || gid.is_null() {
        return crate::ffi::ret::int(Err(fault()));
    }
    match with_net(|net| net.getpeereid(fd)) {
        Ok((peer_uid, peer_gid)) => {
            // SAFETY: the caller supplied two writable uid_t/gid_t slots.
            unsafe {
                uid.write(peer_uid);
                gid.write(peer_gid);
            }
            0
        }
        Err(error) => crate::ffi::ret::int(Err(error)),
    }
}
