// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `fstatfs(2)`.
use core::ffi::c_int;

use crate::platform::statfs::CStatfs;

/// Fill `buf` with the filesystem statistics of the open `fd`.
///
/// # Safety
/// `buf` is NULL or a writable struct statfs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fstatfs(fd: c_int, buf: *mut CStatfs) -> c_int {
    match crate::ffi::platform::global().fstatfs(fd) {
        Ok(wire) => crate::platform::statfs::fill(
            buf,
            wire.block_size,
            wire.blocks,
            wire.free_blocks,
            wire.files,
            wire.free_files,
        ),
        Err(error) => crate::ffi::ret::int(Err(crate::platform::map_error(error))),
    }
}
