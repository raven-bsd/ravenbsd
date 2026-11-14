// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getmntinfo`/`getfsstat` over the `PG-37` seam.

use super::*;
use crate::testutil::FakeMounts;
use alloc::vec;

fn entry(bsize: u64) -> StatfsLinux {
    StatfsLinux {
        f_type: 1,
        f_bsize: bsize,
        f_blocks: 100,
        f_bfree: 40,
        f_bavail: 30,
        f_files: 10,
        f_ffree: 5,
        f_fsid: [1, 2],
        f_namelen: 255,
        f_frsize: 4096,
        f_flags: 0,
        f_spare: [0; 4],
    }
}

#[test]
fn struct_is_120_bytes() {
    assert_eq!(core::mem::size_of::<StatfsLinux>(), 120);
}

#[test]
fn getmntinfo_needs_pg37() {
    let mut mounts = FakeMounts::new();
    assert_eq!(getmntinfo(&mut mounts, MNT_NOWAIT), Err(Errno::ENOSYS));
}

#[test]
fn bad_flag_is_einval() {
    let mut mounts = FakeMounts::new();
    assert_eq!(getmntinfo(&mut mounts, 9), Err(Errno::EINVAL));
    assert_eq!(getfsstat(&mut mounts, None, 9), Err(Errno::EINVAL));
}

#[test]
fn accepted_flags_are_ignored() {
    let mut mounts = FakeMounts::new();
    mounts.mounts = Some(vec![entry(4096)]);
    for flags in [MNT_WAIT, MNT_NOWAIT, MNT_LAZY] {
        assert_eq!(getmntinfo(&mut mounts, flags), Ok(vec![entry(4096)]));
    }
}

#[test]
fn getfsstat_null_buffer_returns_count() {
    let mut mounts = FakeMounts::new();
    mounts.mounts = Some(vec![entry(4096), entry(1024)]);
    assert_eq!(getfsstat(&mut mounts, None, MNT_NOWAIT), Ok(2));
}

#[test]
fn getfsstat_copies_into_the_buffer() {
    let mut mounts = FakeMounts::new();
    mounts.mounts = Some(vec![entry(4096), entry(1024)]);
    let mut buffer = [StatfsLinux::default(); 1];
    assert_eq!(getfsstat(&mut mounts, Some(&mut buffer), MNT_WAIT), Ok(2));
    assert_eq!(buffer[0], entry(4096));
}
