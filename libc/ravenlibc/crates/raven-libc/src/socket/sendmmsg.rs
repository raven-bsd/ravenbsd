// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sendmmsg`.

use core::ffi::{c_int, c_void};

use super::*;

/// `sendmmsg(2)`.
///
/// # Safety
/// `messages` points to `count` writable `mmsghdr`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sendmmsg(
    fd: c_int,
    messages: *mut c_void,
    count: usize,
    flags: c_int,
) -> isize {
    if messages.is_null() && count != 0 {
        return crate::ffi::ret::ssize(Err(fault()));
    }
    let mut sent = 0usize;
    while sent < count {
        // SAFETY: the caller's array of mmsghdr.
        let message = unsafe { messages.cast::<Mmsghdr>().add(sent) };
        // SAFETY: the current mmsghdr is writable.
        let length = unsafe {
            super::socketpair::sendmsg(fd, core::ptr::addr_of!((*message).msg_hdr), flags)
        };
        if length < 0 {
            if sent == 0 {
                return length;
            }
            break;
        }
        // SAFETY: the current mmsghdr is writable.
        unsafe { (*message).msg_len = i64::try_from(length).unwrap_or(i64::MAX) };
        sent = sent.saturating_add(1);
    }
    isize::try_from(sent).unwrap_or(isize::MAX)
}
