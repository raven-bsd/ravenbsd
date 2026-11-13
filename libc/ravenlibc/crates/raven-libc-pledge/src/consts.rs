// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD Capsicum right numbers.

/// Index bit for `cr_rights[0]`.
pub const CAP_INDEX0: u64 = 1 << 57;
/// Index bit for `cr_rights[1]`.
pub const CAP_INDEX1: u64 = 1 << 58;
/// Version 0 in the top two bits of `cr_rights[0]`.
pub const CAP_RIGHTS_VERSION: u64 = 0;

/// Compose one right.
#[must_use]
pub const fn capright(index: u32, bit: u64) -> u64 {
    let index_bit = match index {
        0 => CAP_INDEX0,
        1 => CAP_INDEX1,
        _ => 0,
    };
    index_bit | bit
}

/// `CAP_READ`.
pub const CAP_READ: u64 = capright(0, 0x1);
/// `CAP_WRITE`.
pub const CAP_WRITE: u64 = capright(0, 0x2);
/// `CAP_SEEK_TELL`.
pub const CAP_SEEK_TELL: u64 = capright(0, 0x4);
/// `CAP_SEEK`.
pub const CAP_SEEK: u64 = CAP_SEEK_TELL | 0x8;
/// `CAP_PREAD`.
pub const CAP_PREAD: u64 = CAP_SEEK | CAP_READ;
/// `CAP_PWRITE`.
pub const CAP_PWRITE: u64 = CAP_SEEK | CAP_WRITE;
/// `CAP_MMAP`.
pub const CAP_MMAP: u64 = capright(0, 0x10);
/// `CAP_MMAP_R`.
pub const CAP_MMAP_R: u64 = CAP_MMAP | CAP_SEEK | CAP_READ;
/// `CAP_MMAP_W`.
pub const CAP_MMAP_W: u64 = CAP_MMAP | CAP_SEEK | CAP_WRITE;
/// `CAP_MMAP_X`.
pub const CAP_MMAP_X: u64 = CAP_MMAP | CAP_SEEK | 0x20;
/// `CAP_CREATE`.
pub const CAP_CREATE: u64 = capright(0, 0x40);
/// `CAP_FEXECVE`.
pub const CAP_FEXECVE: u64 = capright(0, 0x80);
/// `CAP_FSYNC`.
pub const CAP_FSYNC: u64 = capright(0, 0x100);
/// `CAP_FTRUNCATE`.
pub const CAP_FTRUNCATE: u64 = capright(0, 0x200);
/// `CAP_FSTAT`.
pub const CAP_FSTAT: u64 = capright(0, 0x80000);
/// `CAP_ALL0`.
pub const CAP_ALL0: u64 = capright(0, 0x0FFF_FFFF_FFFF);
/// `CAP_ALL1`.
pub const CAP_ALL1: u64 = capright(1, 0x1FF_FFFF);
