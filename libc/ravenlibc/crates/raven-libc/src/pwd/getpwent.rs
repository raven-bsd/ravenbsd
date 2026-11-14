// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getpwent(3)`.
use raven_libc_abi::structs::Passwd;

/// The next entry of `/etc/passwd`, loaded on the first call.
#[unsafe(no_mangle)]
pub extern "C" fn getpwent() -> *mut Passwd {
    let loaded =
        crate::ffi::tcbbuf::with_area(|area| crate::pwd::cursor::loaded(area.passwd.cursor));
    if !loaded {
        match crate::pwd::lookup::passwd_entries() {
            Ok(entries) => crate::ffi::tcbbuf::with_area(|area| {
                crate::pwd::cursor::store::<raven_libc_pwd::Passwd>(
                    &mut area.passwd.cursor,
                    entries,
                )
            }),
            Err(error) => return crate::ffi::ret::null(raven_libc_abi::errno::Errno(error as u16)),
        }
    }
    let entry = crate::ffi::tcbbuf::with_area(|area| {
        crate::pwd::cursor::next::<raven_libc_pwd::Passwd>(area.passwd.cursor)
    });
    match entry {
        Some(entry) => {
            crate::ffi::tcbbuf::with_area(|area| {
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
            })
        }
        None => core::ptr::null_mut(),
    }
}
