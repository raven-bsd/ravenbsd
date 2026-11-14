// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getpwnam(3)`.
use core::ffi::c_char;

use raven_libc_abi::errno::{EFAULT, Errno};
use raven_libc_abi::structs::Passwd;

/// Look up one login name in the scratch area.
///
/// # Safety
/// `name` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getpwnam(name: *const c_char) -> *mut Passwd {
    if name.is_null() {
        return crate::ffi::ret::null(Errno(EFAULT as u16));
    }
    // SAFETY: the caller's NUL-terminated name.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    match crate::pwd::lookup::passwd_by_name(name) {
        Ok(Some(entry)) => crate::ffi::tcbbuf::with_area(|area| {
            match crate::ffi::tcbbuf::fill_passwd(
                &entry,
                &mut area.passwd.record,
                &mut area.passwd.buf,
            ) {
                Ok(()) => &raw mut area.passwd.record,
                Err(error) => {
                    crate::ffi::errno::set(error);
                    core::ptr::null_mut()
                }
            }
        }),
        Ok(None) => core::ptr::null_mut(),
        Err(error) => crate::ffi::ret::null(Errno(error as u16)),
    }
}
