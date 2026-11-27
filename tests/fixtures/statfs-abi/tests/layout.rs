// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host assertion for S775-R01: the uutils `libc::statfs` record is the native FreeBSD
//! `struct statfs` that `libc/ravenlibc/crates/raven-libc/src/platform/statfs.rs` writes:
//! 2344 bytes, alignment 8, native field offsets. The pre-S775 record is 120 bytes.

use core::mem::{align_of, offset_of, size_of};

use raven_statfs_abi::record::statfs;

#[test]
fn size_is_the_native_2344_bytes() {
    assert_eq!(size_of::<statfs>(), 2344);
}

#[test]
fn alignment_is_eight() {
    assert_eq!(align_of::<statfs>(), 8);
}

#[test]
fn native_field_offsets() {
    assert_eq!(offset_of!(statfs, f_version), 0);
    assert_eq!(offset_of!(statfs, f_type), 4);
    assert_eq!(offset_of!(statfs, f_flags), 8);
    assert_eq!(offset_of!(statfs, f_bsize), 16);
    assert_eq!(offset_of!(statfs, f_iosize), 24);
    assert_eq!(offset_of!(statfs, f_blocks), 32);
    assert_eq!(offset_of!(statfs, f_bfree), 40);
    assert_eq!(offset_of!(statfs, f_bavail), 48);
    assert_eq!(offset_of!(statfs, f_files), 56);
    assert_eq!(offset_of!(statfs, f_ffree), 64);
    assert_eq!(offset_of!(statfs, f_namemax), 184);
    assert_eq!(offset_of!(statfs, f_owner), 188);
    assert_eq!(offset_of!(statfs, f_fsid), 192);
    assert_eq!(offset_of!(statfs, f_fstypename), 280);
    assert_eq!(offset_of!(statfs, f_mntfromname), 296);
    assert_eq!(offset_of!(statfs, f_mntonname), 1320);
}
