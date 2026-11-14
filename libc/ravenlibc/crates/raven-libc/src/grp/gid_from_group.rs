// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `gid_from_group(3)`.
use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EINVAL, ENOENT, Errno};
use raven_libc_pwd::{FilesSource, PwCache};

/// Look up one group name and store its gid.
///
/// # Safety
/// `name` is NUL-terminated; `gid` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn gid_from_group(name: *const c_char, gid: *mut u32) -> c_int {
    if name.is_null() || gid.is_null() {
        return crate::ffi::ret::int(Err(Errno(EINVAL as u16)));
    }
    // SAFETY: the caller's NUL-terminated name.
    let name = unsafe { crate::ffi::cstr::slice(name) };
    let Ok(name) = core::str::from_utf8(name) else {
        return crate::ffi::ret::int(Err(Errno(ENOENT as u16)));
    };
    let source = FilesSource::new(&crate::pwd::source::ShellFiles);
    let cache = PwCache::new(&source, &source);
    match cache.gid_from_group(name) {
        Ok(Some(value)) => {
            // SAFETY: the caller's writable gid slot.
            unsafe { gid.write(value) };
            0
        }
        Ok(None) => crate::ffi::ret::int(Err(Errno(ENOENT as u16))),
        Err(error) => crate::ffi::ret::int(Err(Errno(error as u16))),
    }
}
