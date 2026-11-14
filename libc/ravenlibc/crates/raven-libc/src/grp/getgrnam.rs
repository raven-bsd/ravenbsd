// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getgrnam(3)`.
use core::ffi::c_char;

use raven_libc_abi::errno::{EFAULT, Errno};
use raven_libc_abi::structs::Group;

/// Look up one group name in the scratch area.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgrnam(name: *const c_char) -> *mut Group {
    if name.is_null() {
        return crate::ffi::ret::null(Errno(EFAULT as u16));
    }
    // SAFETY: the caller's NUL-terminated name.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    match crate::pwd::lookup::group_by_name(name) {
        Ok(Some(entry)) => {
            crate::ffi::tcbbuf::with_area(|area| {
                match crate::ffi::tcbbuf::fill_group(
                    &entry,
                    &mut area.group.record,
                    &mut area.group.buf,
                ) {
                    Ok(()) => &raw mut area.group.record,
                    Err(error) => {
                        crate::ffi::errno::set(error);
                        core::ptr::null_mut()
                    }
                }
            })
        }
        Ok(None) => core::ptr::null_mut(),
        Err(error) => crate::ffi::ret::null(Errno(error as u16)),
    }
}
