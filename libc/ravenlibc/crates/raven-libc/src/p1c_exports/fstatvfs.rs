// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `fstatvfs`.

use super::*;

/// Read file-system statistics for an open descriptor.
///
/// # Safety
/// `buf` points to a writable `struct statvfs`.
#[cfg_attr(target_os = "none", unsafe(no_mangle))]
pub unsafe extern "C" fn fstatvfs(fd: c_int, buf: *mut Statvfs) -> c_int {
    if buf.is_null() {
        return ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    match global().fstatvfs(fd) {
        Ok(value) => {
            // SAFETY: the caller supplied a writable Statvfs.
            unsafe { *buf = value };
            0
        }
        Err(error) => ret::int(Err(map_error(error))),
    }
}
