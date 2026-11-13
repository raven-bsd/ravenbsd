// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `struct stat` and `struct statvfs` conversion tests.

use raven_proto_fs::{Stat as WireStat, StatFs};

use crate::stat::{STATX_BASIC_MASK, StatSource, from_wire, statvfs_from_wire};

fn record() -> WireStat {
    WireStat {
        ino: 42,
        size: 1234,
        blocks: 3,
        mtime: 100,
        ctime: 101,
        atime: 102,
        mode: 0o100_644,
        nlink: 2,
        uid: 1000,
        gid: 1001,
    }
}

#[test]
fn fields_copy_one_to_one() {
    let stat = from_wire(&record(), StatSource::File);
    assert_eq!(stat.st_ino, 42);
    assert_eq!(stat.st_size, 1234);
    assert_eq!(stat.st_blocks, 3);
    assert_eq!(stat.st_mode, 0o100_644);
    assert_eq!(stat.st_nlink, 2);
    assert_eq!(stat.st_uid, 1000);
    assert_eq!(stat.st_gid, 1001);
}

#[test]
fn times_are_whole_seconds() {
    let stat = from_wire(&record(), StatSource::File);
    assert_eq!(stat.st_atim.tv_sec, 102);
    assert_eq!(stat.st_mtim.tv_sec, 100);
    assert_eq!(stat.st_ctim.tv_sec, 101);
    assert_eq!(stat.st_atim.tv_nsec, 0);
}

#[test]
fn blocks_are_in_512_byte_units() {
    let mut wire = record();
    wire.blocks = 8;
    let stat = from_wire(&wire, StatSource::File);
    assert_eq!(stat.st_blocks, 8);
}

#[test]
fn dev_rdev_and_blksize_are_the_documented_defaults() {
    let stat = from_wire(&record(), StatSource::File);
    assert_eq!(stat.st_dev, 0);
    assert_eq!(stat.st_rdev, 0);
    assert_eq!(stat.st_blksize, 4096);
}

#[test]
fn missing_type_bits_are_filled_from_the_source() {
    let mut wire = record();
    wire.mode = 0o644;
    assert_eq!(
        from_wire(&wire, StatSource::Dir).st_mode & raven_libc_abi::stat::S_IFMT,
        raven_libc_abi::stat::S_IFDIR
    );
    assert_eq!(
        from_wire(&wire, StatSource::Tty).st_mode & raven_libc_abi::stat::S_IFMT,
        raven_libc_abi::stat::S_IFCHR
    );
    assert_eq!(
        from_wire(&wire, StatSource::Pipe).st_mode & raven_libc_abi::stat::S_IFMT,
        raven_libc_abi::stat::S_IFIFO
    );
    assert_eq!(
        from_wire(&wire, StatSource::Socket).st_mode & raven_libc_abi::stat::S_IFMT,
        raven_libc_abi::stat::S_IFSOCK
    );
}

#[test]
fn statvfs_maps_the_wire_record() {
    let wire = StatFs {
        block_size: 4096,
        blocks: 1000,
        free_blocks: 250,
        files: 100,
        free_files: 25,
    };
    let statvfs = statvfs_from_wire(&wire);
    assert_eq!(statvfs.f_bsize, 4096);
    assert_eq!(statvfs.f_frsize, 4096);
    assert_eq!(statvfs.f_blocks, 1000);
    assert_eq!(statvfs.f_bfree, 250);
    assert_eq!(statvfs.f_bavail, 250);
    assert_eq!(statvfs.f_files, 100);
    assert_eq!(statvfs.f_ffree, 25);
    assert_eq!(statvfs.f_namemax, 255);
    assert_eq!(statvfs.f_type, 0);
}

#[test]
fn statx_never_claims_btime() {
    // STATX_BTIME is bit 0x800; the basic mask must not include it.
    assert_eq!(STATX_BASIC_MASK & 0x800, 0);
    assert_ne!(STATX_BASIC_MASK & 0x7ff, 0);
}
