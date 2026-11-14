// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sendmmsg`.

use core::ffi::{c_int, c_uint, c_void};

use super::*;

/// `sendmmsg(2)`.
///
/// # Safety
/// `messages` points to `count` writable `mmsghdr`s.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sendmmsg(
    fd: c_int,
    messages: *mut c_void,
    count: c_uint,
    flags: c_int,
) -> c_int {
    if messages.is_null() && count != 0 {
        return crate::ffi::ret::int(Err(fault()));
    }
    let mut sent = 0u32;
    while sent < count {
        // SAFETY: the caller's array of mmsghdr.
        let message = unsafe { messages.cast::<Mmsghdr>().add(sent as usize) };
        // SAFETY: the current mmsghdr is writable.
        let length = unsafe {
            super::socketpair::sendmsg(fd, core::ptr::addr_of!((*message).header), flags)
        };
        if length < 0 {
            if sent == 0 {
                return length as c_int;
            }
            break;
        }
        // SAFETY: the current mmsghdr is writable.
        unsafe { (*message).len = u32::try_from(length).unwrap_or(u32::MAX) };
        sent = sent.saturating_add(1);
    }
    c_int::try_from(sent).unwrap_or(c_int::MAX)
}
