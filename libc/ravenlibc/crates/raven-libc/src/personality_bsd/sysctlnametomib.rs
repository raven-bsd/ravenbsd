// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `sysctlnametomib(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_personality_bsd::seams::Errno;

use crate::personality_common::ret_unit;

/// `sysctlnametomib(name, mib, miblenp)`.
///
/// # Safety
/// `name` is NUL terminated; `mib` has `*miblenp` writable entries; `miblenp` is writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn sysctlnametomib(
    name: *const c_char,
    mib: *mut c_int,
    miblenp: *mut usize,
) -> c_int {
    if name.is_null() || mib.is_null() || miblenp.is_null() {
        return ret_unit(Err(Errno::EFAULT));
    }
    // SAFETY: the caller's name contract.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    // SAFETY: the caller granted one readable size_t.
    let mut length = unsafe { core::ptr::read_unaligned(miblenp) };
    if length == 0 {
        return ret_unit(Err(Errno::ENOMEM));
    }
    // SAFETY: the caller granted `length` writable int entries.
    let out = unsafe { core::slice::from_raw_parts_mut(mib, length) };
    let result = raven_libc_personality_bsd::sysctl::sysctlnametomib(name, out, &mut length);
    // SAFETY: the caller granted one writable size_t.
    unsafe { core::ptr::write_unaligned(miblenp, length) };
    ret_unit(result)
}
