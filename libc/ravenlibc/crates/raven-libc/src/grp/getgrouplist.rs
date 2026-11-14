// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `getgrouplist(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EFAULT, EINVAL, Errno};
use raven_libc_pwd::{FilesSource, GrpListError};

/// Fill the caller's group list and report the count in `*ngroups`.
///
/// # Safety
/// Every pointer follows its C contract.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn getgrouplist(
    user: *const c_char,
    gid: u32,
    groups: *mut u32,
    ngroups: *mut c_int,
) -> c_int {
    if user.is_null() || ngroups.is_null() {
        return crate::ffi::ret::int(Err(Errno(EINVAL as u16)));
    }
    // SAFETY: the caller's writable count slot.
    let size = unsafe { ngroups.read() };
    if size < 0 {
        return crate::ffi::ret::int(Err(Errno(EINVAL as u16)));
    }
    if groups.is_null() && size != 0 {
        return crate::ffi::ret::int(Err(Errno(EFAULT as u16)));
    }
    // SAFETY: the caller's name and writable group slots.
    let (user, out) = unsafe {
        (
            crate::ffi::cstr::slice(user),
            if size == 0 {
                &mut [][..]
            } else {
                core::slice::from_raw_parts_mut(groups, usize::try_from(size).unwrap_or(0))
            },
        )
    };
    let Ok(user) = core::str::from_utf8(user) else {
        return crate::ffi::ret::int(Err(Errno(EINVAL as u16)));
    };
    let source = FilesSource::new(&crate::pwd::source::ShellFiles);
    match raven_libc_pwd::getgrouplist(user, gid, &source, out) {
        Ok(count) => {
            // SAFETY: the caller's writable count slot.
            unsafe { ngroups.write(c_int::try_from(count).unwrap_or(c_int::MAX)) };
            c_int::try_from(count).unwrap_or(c_int::MAX)
        }
        Err(GrpListError::TooSmall { needed }) => {
            // SAFETY: the caller's writable count slot.
            unsafe { ngroups.write(c_int::try_from(needed).unwrap_or(c_int::MAX)) };
            crate::ffi::ret::int(Err(Errno(EINVAL as u16)))
        }
        Err(GrpListError::Io(error)) => crate::ffi::ret::int(Err(Errno(error as u16))),
    }
}
