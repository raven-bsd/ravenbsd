// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `statfs(2)`.
//!
//! The public record is the FreeBSD `struct statfs` selected by DR-69/C0 (2344 bytes), not the
//! musl/Linux shape. Fields the 64-byte `StatFs` wire cannot answer stay zero: the filesystem
//! type, mount flags, string names, owner and counters are not invented.
use core::ffi::{c_char, c_int};

/// The donor record version word (`freebsd:sys/sys/mount.h` `STATFS_VERSION`).
pub const STATFS_VERSION: u32 = 0x2014_0518;

/// The C `struct statfs` of `sys/statfs.h` (FreeBSD profile).
#[repr(C)]
pub struct CStatfs {
    f_version: u32,
    f_type: u32,
    f_flags: u64,
    f_bsize: u64,
    f_iosize: u64,
    f_blocks: u64,
    f_bfree: u64,
    f_bavail: i64,
    f_files: u64,
    f_ffree: i64,
    f_syncwrites: u64,
    f_asyncwrites: u64,
    f_syncreads: u64,
    f_asyncreads: u64,
    f_nvnodelistsize: u32,
    f_spare0: u32,
    f_spare: [u64; 9],
    f_namemax: u32,
    f_owner: u32,
    f_fsid: [i32; 2],
    f_charspare: [c_char; 80],
    f_fstypename: [c_char; 16],
    f_mntfromname: [c_char; 1024],
    f_mntonname: [c_char; 1024],
}

const _: () = assert!(core::mem::size_of::<CStatfs>() == 2344);
const _: () = assert!(core::mem::align_of::<CStatfs>() == 8);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_version) == 0);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_type) == 4);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_flags) == 8);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_bsize) == 16);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_iosize) == 24);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_blocks) == 32);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_bfree) == 40);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_bavail) == 48);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_files) == 56);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_ffree) == 64);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_syncwrites) == 72);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_asyncwrites) == 80);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_syncreads) == 88);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_asyncreads) == 96);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_nvnodelistsize) == 104);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_spare0) == 108);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_spare) == 112);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_namemax) == 184);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_owner) == 188);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_fsid) == 192);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_charspare) == 200);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_fstypename) == 280);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_mntfromname) == 296);
const _: () = assert!(core::mem::offset_of!(CStatfs, f_mntonname) == 1320);

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
        f_version: STATFS_VERSION,
        f_type: 0,
        f_flags: 0,
        f_bsize: u64::from(block_size),
        f_iosize: u64::from(block_size),
        f_blocks: blocks,
        f_bfree: free_blocks,
        // G-19 adds `bavail`; until then free blocks is the honest lower bound.
        f_bavail: free_blocks as i64,
        f_files: files,
        f_ffree: free_files as i64,
        f_syncwrites: 0,
        f_asyncwrites: 0,
        f_syncreads: 0,
        f_asyncreads: 0,
        f_nvnodelistsize: 0,
        f_spare0: 0,
        f_spare: [0; 9],
        f_namemax: 255,
        f_owner: 0,
        f_fsid: [0, 0],
        f_charspare: [0; 80],
        f_fstypename: [0; 16],
        f_mntfromname: [0; 1024],
        f_mntonname: [0; 1024],
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
