// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `statfs(2)`.
use core::ffi::{c_char, c_int, c_ulong};

/// The C `struct statfs` of `sys/statfs.h` (musl layout).
#[repr(C)]
pub struct CStatfs {
    f_type: c_ulong,
    f_bsize: c_ulong,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: u64,
    f_files: u64,
    f_ffree: u64,
    f_fsid: [c_int; 2],
    f_namelen: c_ulong,
    f_frsize: c_ulong,
    f_flags: c_ulong,
    f_spare: [c_ulong; 4],
}

pub(crate) fn fill(
    buffer: *mut CStatfs,
    block_size: u32,
    blocks: u64,
    free_blocks: u64,
    files: u64,
    free_files: u64,
) -> c_int {
    if buffer.is_null() {
        return crate::ffi::ret::int(Err(raven_libc_abi::errno::Errno(
            raven_libc_abi::errno::EFAULT as u16,
        )));
    }
    let value = CStatfs {
        f_type: 0,
        f_bsize: c_ulong::from(block_size),
        f_blocks: blocks,
        f_bfree: free_blocks,
        f_bavail: free_blocks,
        f_files: files,
        f_ffree: free_files,
        f_fsid: [0, 0],
        f_namelen: 255,
        f_frsize: c_ulong::from(block_size),
        f_flags: 0,
        f_spare: [0; 4],
    };
    // SAFETY: `buffer` is the caller's writable struct statfs.
    unsafe { core::ptr::write(buffer, value) };
    0
}

/// Fill `buf` with the filesystem statistics of `path`.
///
/// # Safety
/// `path` is NULL or a NUL-terminated string; `buf` is NULL or a writable struct statfs.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn statfs(path: *const c_char, buf: *mut CStatfs) -> c_int {
    // SAFETY: the caller's C string contract.
    let path = unsafe { crate::ffi::cstr::slice(path) };
    match crate::ffi::platform::global().statfs(path) {
        Ok(wire) => fill(
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
