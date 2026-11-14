// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sysctlbyname(3)`.
use core::ffi::{c_char, c_int, c_void};

use raven_libc_personality_bsd::seams::Errno;

use crate::personality_common::{RealSysInfoBsd, ret_unit};

/// `sysctlbyname(name, old, oldlenp, new, newlen)`.
///
/// # Safety
/// `name` is NUL terminated; the pointer/length pairs are valid, or NULL with zero length.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysctlbyname(
    name: *const c_char,
    old: *mut c_void,
    oldlenp: *mut usize,
    new: *const c_void,
    newlen: usize,
) -> c_int {
    if name.is_null() || (new.is_null() && newlen > 0) {
        return ret_unit(Err(Errno::EFAULT));
    }
    // SAFETY: the caller's name contract.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    // SAFETY: the caller granted `newlen` readable bytes when non-NULL.
    let new = if newlen == 0 {
        None
    } else {
        Some(unsafe { core::slice::from_raw_parts(new.cast::<u8>(), newlen) })
    };
    if old.is_null() {
        let mut length = if oldlenp.is_null() {
            0
        } else {
            // SAFETY: the caller granted one readable size_t.
            unsafe { core::ptr::read_unaligned(oldlenp) }
        };
        let result = raven_libc_personality_bsd::sysctl::sysctlbyname(
            &mut RealSysInfoBsd,
            name,
            None,
            &mut length,
            new,
        );
        if result.is_ok() && !oldlenp.is_null() {
            // SAFETY: the caller granted one writable size_t.
            unsafe { core::ptr::write_unaligned(oldlenp, length) };
        }
        return ret_unit(result);
    }
    if oldlenp.is_null() {
        return ret_unit(Err(Errno::EFAULT));
    }
    // SAFETY: the caller granted one readable/writable size_t.
    let mut length = unsafe { core::ptr::read_unaligned(oldlenp) };
    // SAFETY: the caller granted `length` writable bytes.
    let buffer = unsafe { core::slice::from_raw_parts_mut(old.cast::<u8>(), length) };
    let result = raven_libc_personality_bsd::sysctl::sysctlbyname(
        &mut RealSysInfoBsd,
        name,
        Some(buffer),
        &mut length,
        new,
    );
    // SAFETY: the caller granted one writable size_t.
    unsafe { core::ptr::write_unaligned(oldlenp, length) };
    ret_unit(result)
}
