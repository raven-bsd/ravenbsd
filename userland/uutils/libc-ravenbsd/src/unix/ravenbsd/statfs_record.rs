// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The RavenBSD `struct statfs` ABI record.
//!
//! Kept in its own file so the target libc module and the host layout test compile the same
//! definition: the record is written by the native libc with `write(buffer, value)`, so the
//! caller's type must be exactly this layout.

use core::ffi::c_int;

/// The Linux-shaped record this crate shipped before S775 (120 bytes). It is not the record the
/// native `statfs` writes.
#[repr(C)]
#[derive(Copy, Clone, Debug)]
pub struct statfs {
    pub f_type: u64,
    pub f_bsize: u64,
    pub f_blocks: u64,
    pub f_bfree: u64,
    pub f_bavail: u64,
    pub f_files: u64,
    pub f_ffree: u64,
    pub f_fsid: [c_int; 2],
    pub f_namelen: u64,
    pub f_frsize: u64,
    pub f_flags: u64,
    pub f_spare: [u64; 4],
}
