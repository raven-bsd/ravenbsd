// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getmntinfo(3)`.
use core::ffi::c_int;

use raven_libc_personality_bsd::getmntinfo::StatfsLinux;
use raven_libc_personality_bsd::seams::Errno;

use crate::personality_common::RealMounts;

/// Fill `*mntbufp` with the mount list and return its length; `ENOSYS` until PG-37 returns 0
/// with `errno` set, the BSD convention for "no entries".
///
/// # Safety
/// `mntbufp` points to one writable pointer.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getmntinfo(mntbufp: *mut *mut StatfsLinux, flags: c_int) -> c_int {
    if mntbufp.is_null() {
        crate::ffi::errno::set_platform(Errno::EFAULT);
        return -1;
    }
    match raven_libc_personality_bsd::getmntinfo::getmntinfo(&mut RealMounts, flags) {
        Ok(list) => {
            let count = list.len();
            let boxed = list.into_boxed_slice();
            let pointer = alloc::boxed::Box::into_raw(boxed);
            // SAFETY: the caller granted one writable pointer.
            unsafe { core::ptr::write_unaligned(mntbufp, pointer.cast::<StatfsLinux>()) };
            c_int::try_from(count).unwrap_or(0)
        }
        Err(error) => {
            crate::ffi::errno::set_platform(error);
            0
        }
    }
}
