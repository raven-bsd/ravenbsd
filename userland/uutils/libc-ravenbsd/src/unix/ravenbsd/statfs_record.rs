// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The RavenBSD `struct statfs` ABI record.
//!
//! This is the FreeBSD `sys/sys/mount.h` record the native libc writes (2344 bytes, alignment 8),
//! not the 120-byte Linux shape. It is kept in its own file so the target libc module and the
//! host layout test compile the same definition: the native writer writes exactly
//! `sizeof(struct statfs)` into the caller's buffer.
//!
//! The fourteen counters are followed by the ten reserved `uint64_t` words that the C record
//! spells as `f_nvnodelistsize`, `f_spare0` and `f_spare[9]` over the same 80 bytes; the public
//! libc record does not expose those internals.

use core::ffi::{c_char, c_int};

/// The FreeBSD `struct statfs` (`sys/sys/mount.h`, donor pin `133344405`).
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct statfs {
    pub f_version: u32,
    pub f_type: u32,
    pub f_flags: u64,
    pub f_bsize: u64,
    pub f_iosize: u64,
    pub f_blocks: u64,
    pub f_bfree: u64,
    pub f_bavail: i64,
    pub f_files: u64,
    pub f_ffree: i64,
    pub f_syncwrites: u64,
    pub f_asyncwrites: u64,
    pub f_syncreads: u64,
    pub f_asyncreads: u64,
    f_spare: [u64; 10],
    pub f_namemax: u32,
    pub f_owner: u32,
    pub f_fsid: [c_int; 2],
    f_charspare: [c_char; 80],
    pub f_fstypename: [c_char; 16],
    pub f_mntfromname: [c_char; 1024],
    pub f_mntonname: [c_char; 1024],
}
