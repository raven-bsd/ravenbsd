// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getgrent(3)`.
use raven_libc_abi::structs::Group;

/// The next entry of `/etc/group`, loaded on the first call.
#[unsafe(no_mangle)]
pub extern "C" fn getgrent() -> *mut Group {
    let loaded =
        crate::ffi::tcbbuf::with_area(|area| crate::pwd::cursor::loaded(area.group.cursor));
    if !loaded {
        match crate::pwd::lookup::group_entries() {
            Ok(entries) => crate::ffi::tcbbuf::with_area(|area| {
                crate::pwd::cursor::store::<raven_libc_pwd::Group>(&mut area.group.cursor, entries)
            }),
            Err(error) => return crate::ffi::ret::null(raven_libc_abi::errno::Errno(error as u16)),
        }
    }
    let entry = crate::ffi::tcbbuf::with_area(|area| {
        crate::pwd::cursor::next::<raven_libc_pwd::Group>(area.group.cursor)
    });
    match entry {
        Some(entry) => crate::ffi::tcbbuf::with_area(|area| {
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
        }),
        None => core::ptr::null_mut(),
    }
}
