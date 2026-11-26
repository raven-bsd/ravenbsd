// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sysctlbyname(3)`.
use core::ffi::{c_char, c_int, c_void};

use raven_libc_personality_bsd::seams::Errno;

use crate::personality_common::{RealSysInfoBsd, ret_unit};

/// Write `*oldlenp` only for the results where FreeBSD copies it out.
fn copy_out(oldlenp: *mut usize, length: usize, result: Result<(), Errno>) {
    if !oldlenp.is_null() && (result.is_ok() || matches!(result, Err(Errno::ENOMEM))) {
        // SAFETY: the caller granted one writable size_t.
        unsafe { core::ptr::write_unaligned(oldlenp, length) };
    }
}

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
    if name.is_null() {
        return ret_unit(Err(Errno::EFAULT));
    }
    // SAFETY: the caller's name contract.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    // SAFETY: a non-NULL `new` grants `newlen` readable bytes; a NULL `new` means no write,
    // whatever `newlen` says (FreeBSD `userland_sysctl` ignores `newlen` when `new` is NULL).
    let new = if new.is_null() {
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
        copy_out(oldlenp, length, result);
        return ret_unit(result);
    }
    if oldlenp.is_null() {
        // FreeBSD treats a missing length as zero: the output is ENOMEM and `oldlenp` is not
        // written (it is NULL), not EFAULT.
        let mut empty: [u8; 0] = [];
        let mut length = 0usize;
        let result = raven_libc_personality_bsd::sysctl::sysctlbyname(
            &mut RealSysInfoBsd,
            name,
            Some(&mut empty[..]),
            &mut length,
            new,
        );
        return ret_unit(result);
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
    copy_out(oldlenp, length, result);
    ret_unit(result)
}
