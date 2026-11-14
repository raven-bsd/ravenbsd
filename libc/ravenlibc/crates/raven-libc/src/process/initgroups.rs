// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `initgroups(3)`.
use core::ffi::{c_char, c_int};

/// Build the user's group list from the group database and install it.
///
/// # Safety
/// `user` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn initgroups(user: *const c_char, gid: u32) -> c_int {
    if user.is_null() {
        return crate::ffi::ret::int(Err(crate::process::exec_common::fault()));
    }
    // SAFETY: the caller's NUL-terminated user name.
    let name = unsafe { crate::ffi::cstr::slice(user) };
    let Ok(name) = core::str::from_utf8(name) else {
        return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
    };
    let source = raven_libc_pwd::FilesSource::new(&crate::pwd::source::ShellFiles);
    let groups = match raven_libc_pwd::initgroups_groups(name, gid, &source) {
        Ok(groups) => groups,
        Err(raven_libc_pwd::GrpListError::TooSmall { .. }) => {
            return crate::ffi::ret::int(Err(crate::ffi::map_proc::invalid()));
        }
        Err(raven_libc_pwd::GrpListError::Io(error)) => {
            return crate::ffi::ret::int(Err(crate::ffi::map_proc::error(error)));
        }
    };
    let result = crate::ffi::map_proc::setgroups(
        &crate::ffi::procbackend::backend(),
        c_int::try_from(groups.len()).unwrap_or(c_int::MAX),
        groups.as_ptr(),
    );
    crate::ffi::ret::int(result.map(|()| 0).map_err(crate::ffi::map_proc::error))
}
