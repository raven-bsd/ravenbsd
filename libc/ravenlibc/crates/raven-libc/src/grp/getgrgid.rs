// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getgrgid(3)`.
use raven_libc_abi::structs::Group;

/// Look up one group id in the scratch area.
#[unsafe(no_mangle)]
pub extern "C" fn getgrgid(gid: u32) -> *mut Group {
    match crate::pwd::lookup::group_by_gid(gid) {
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
        Err(error) => crate::ffi::ret::null(raven_libc_abi::errno::Errno(error as u16)),
    }
}
