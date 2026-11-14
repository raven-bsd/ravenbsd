// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getpwuid(3)`.
use raven_libc_abi::structs::Passwd;

/// Look up one user id in the scratch area.
#[unsafe(no_mangle)]
pub extern "C" fn getpwuid(uid: u32) -> *mut Passwd {
    match crate::pwd::lookup::passwd_by_uid(uid) {
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
        Err(error) => crate::ffi::ret::null(raven_libc_abi::errno::Errno(error as u16)),
    }
}
