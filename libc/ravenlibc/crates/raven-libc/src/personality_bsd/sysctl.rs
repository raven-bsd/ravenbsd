// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sysctl(2)` (the BSD API; Raven has no syscall).
use core::ffi::{c_int, c_uint, c_void};

use raven_libc_personality_bsd::seams::Errno;

use crate::personality_common::{RealSysInfoBsd, ret_unit};

/// `sysctl(mib, namelen, old, oldlenp, new, newlen)`.
///
/// # Safety
/// The pointer/length pairs are valid, or NULL with zero length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysctl(
    mib: *const c_int,
    namelen: c_uint,
    old: *mut c_void,
    oldlenp: *mut usize,
    new: *const c_void,
    newlen: usize,
) -> c_int {
    let count = usize::try_from(namelen).unwrap_or(0);
    if (mib.is_null() && count > 0) || (new.is_null() && newlen > 0) {
        return ret_unit(Err(Errno::EFAULT));
    }
    // SAFETY: the caller granted `count` readable mib entries.
    let mib = unsafe { core::slice::from_raw_parts(mib, count) };
    // SAFETY: the caller granted `newlen` readable bytes when non-NULL.
    let new = if newlen == 0 {
        None
    } else {
        Some(unsafe { core::slice::from_raw_parts(new.cast::<u8>(), newlen) })
    };
    if old.is_null() {
        if !oldlenp.is_null() {
            // A query for the needed length; the model writes it through `oldlen`.
            let mut length = 0usize;
            let result = raven_libc_personality_bsd::sysctl::sysctl(
                &mut RealSysInfoBsd,
                mib,
                None,
                &mut length,
                new,
            );
            if result.is_ok() {
                // SAFETY: the caller granted one writable size_t.
                unsafe { core::ptr::write_unaligned(oldlenp, length) };
            }
            return ret_unit(result);
        }
        let mut length = 0usize;
        return ret_unit(raven_libc_personality_bsd::sysctl::sysctl(
            &mut RealSysInfoBsd,
            mib,
            None,
            &mut length,
            new,
        ));
    }
    if oldlenp.is_null() {
        return ret_unit(Err(Errno::EFAULT));
    }
    // SAFETY: the caller granted one readable/writable size_t.
    let mut length = unsafe { core::ptr::read_unaligned(oldlenp) };
    // SAFETY: the caller granted `length` writable bytes.
    let buffer = unsafe { core::slice::from_raw_parts_mut(old.cast::<u8>(), length) };
    let result = raven_libc_personality_bsd::sysctl::sysctl(
        &mut RealSysInfoBsd,
        mib,
        Some(buffer),
        &mut length,
        new,
    );
    // SAFETY: the caller granted one writable size_t; the model stores the needed length even
    // on `ENOMEM`.
    unsafe { core::ptr::write_unaligned(oldlenp, length) };
    ret_unit(result)
}
