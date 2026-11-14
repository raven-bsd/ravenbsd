// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getfsstat(2)`.
use core::ffi::c_int;

use raven_libc_personality_bsd::getmntinfo::StatfsLinux;
use raven_libc_personality_bsd::seams::Errno;

use crate::personality_common::{RealMounts, ret_count};

/// Fill the caller's `statfs` array and return the number of entries (or the needed count when
/// `buf` is NULL); `ENOSYS` until PG-37.
///
/// # Safety
/// `buf` is NULL with `bufsize == 0`, or points to `bufsize` writable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getfsstat(buf: *mut StatfsLinux, bufsize: c_int, flags: c_int) -> c_int {
    if bufsize < 0 {
        return ret_count(Err(Errno::EINVAL));
    }
    let bytes = usize::try_from(bufsize).unwrap_or(0);
    let buffer = if buf.is_null() {
        if bytes != 0 {
            return ret_count(Err(Errno::EFAULT));
        }
        None
    } else {
        let count = bytes / core::mem::size_of::<StatfsLinux>();
        // SAFETY: the caller granted `bufsize` writable bytes.
        Some(unsafe { core::slice::from_raw_parts_mut(buf, count) })
    };
    ret_count(
        raven_libc_personality_bsd::getmntinfo::getfsstat(&mut RealMounts, buffer, flags)
            .map(|value| usize::try_from(value).unwrap_or(0)),
    )
}
