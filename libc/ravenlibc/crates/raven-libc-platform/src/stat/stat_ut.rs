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
    assert_eq!(stat.st_mode, 0o100_644u16);
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
    assert_eq!(stat.st_bsdflags, 0);
    assert_eq!(stat.st_padding1, 0);
    assert_eq!(stat.st_flags, 0);
    assert_eq!(stat.st_gen, 0);
    assert_eq!(stat.st_filerev, 0);
    assert_eq!(stat.st_spare, [0; 9]);
    assert_eq!(
        stat.st_birthtim,
        raven_libc_abi::structs::Timespec {
            tv_sec: 0,
            tv_nsec: 0
        }
    );
}

#[test]
fn missing_type_bits_are_filled_from_the_source() {
    let mut wire = record();
    wire.mode = 0o644;
    assert_eq!(
        from_wire(&wire, StatSource::Dir).st_mode & raven_libc_abi::stat::S_IFMT as u16,
        raven_libc_abi::stat::S_IFDIR as u16
    );
    assert_eq!(
        from_wire(&wire, StatSource::Tty).st_mode & raven_libc_abi::stat::S_IFMT as u16,
        raven_libc_abi::stat::S_IFCHR as u16
    );
    assert_eq!(
        from_wire(&wire, StatSource::Pipe).st_mode & raven_libc_abi::stat::S_IFMT as u16,
        raven_libc_abi::stat::S_IFIFO as u16
    );
    assert_eq!(
        from_wire(&wire, StatSource::Socket).st_mode & raven_libc_abi::stat::S_IFMT as u16,
        raven_libc_abi::stat::S_IFSOCK as u16
    );
}

/// The public `struct stat` is the FreeBSD profile at the S700 pin (224 bytes, align 8),
/// not the musl/Linux record it replaced. The numbers below are written from
/// `freebsd:sys/sys/stat.h` and independently re-measured by `hdrgen verify --profile freebsd`.
#[test]
fn public_stat_record_is_the_freebsd_profile() {
    use raven_libc_abi::structs::Stat;

    assert_eq!(core::mem::size_of::<Stat>(), 224);
    assert_eq!(core::mem::align_of::<Stat>(), 8);
    assert_eq!(core::mem::offset_of!(Stat, st_dev), 0);
    assert_eq!(core::mem::offset_of!(Stat, st_ino), 8);
    assert_eq!(core::mem::offset_of!(Stat, st_nlink), 16);
    assert_eq!(core::mem::offset_of!(Stat, st_mode), 24);
    assert_eq!(core::mem::offset_of!(Stat, st_bsdflags), 26);
    assert_eq!(core::mem::offset_of!(Stat, st_uid), 28);
    assert_eq!(core::mem::offset_of!(Stat, st_gid), 32);
    assert_eq!(core::mem::offset_of!(Stat, st_padding1), 36);
    assert_eq!(core::mem::offset_of!(Stat, st_rdev), 40);
    assert_eq!(core::mem::offset_of!(Stat, st_atim), 48);
    assert_eq!(core::mem::offset_of!(Stat, st_mtim), 64);
    assert_eq!(core::mem::offset_of!(Stat, st_ctim), 80);
    assert_eq!(core::mem::offset_of!(Stat, st_birthtim), 96);
    assert_eq!(core::mem::offset_of!(Stat, st_size), 112);
    assert_eq!(core::mem::offset_of!(Stat, st_blocks), 120);
    assert_eq!(core::mem::offset_of!(Stat, st_blksize), 128);
    assert_eq!(core::mem::offset_of!(Stat, st_flags), 132);
    assert_eq!(core::mem::offset_of!(Stat, st_gen), 136);
    assert_eq!(core::mem::offset_of!(Stat, st_filerev), 144);
    assert_eq!(core::mem::offset_of!(Stat, st_spare), 152);
}

/// The public `struct statvfs` is the donor's 88-byte order.
#[test]
fn public_statvfs_record_is_the_freebsd_profile() {
    use raven_libc_abi::structs::Statvfs;

    assert_eq!(core::mem::size_of::<Statvfs>(), 88);
    assert_eq!(core::mem::align_of::<Statvfs>(), 8);
    assert_eq!(core::mem::offset_of!(Statvfs, f_bavail), 0);
    assert_eq!(core::mem::offset_of!(Statvfs, f_bfree), 8);
    assert_eq!(core::mem::offset_of!(Statvfs, f_blocks), 16);
    assert_eq!(core::mem::offset_of!(Statvfs, f_favail), 24);
    assert_eq!(core::mem::offset_of!(Statvfs, f_ffree), 32);
    assert_eq!(core::mem::offset_of!(Statvfs, f_files), 40);
    assert_eq!(core::mem::offset_of!(Statvfs, f_bsize), 48);
    assert_eq!(core::mem::offset_of!(Statvfs, f_flag), 56);
    assert_eq!(core::mem::offset_of!(Statvfs, f_frsize), 64);
    assert_eq!(core::mem::offset_of!(Statvfs, f_fsid), 72);
    assert_eq!(core::mem::offset_of!(Statvfs, f_namemax), 80);
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
    assert_eq!(statvfs.f_favail, 25);
    assert_eq!(statvfs.f_namemax, 255);
    assert_eq!(statvfs.f_fsid, 0);
    assert_eq!(statvfs.f_flag, 0);
}

#[test]
fn statx_never_claims_btime() {
    // STATX_BTIME is bit 0x800; the basic mask must not include it.
    assert_eq!(STATX_BASIC_MASK & 0x800, 0);
    assert_ne!(STATX_BASIC_MASK & 0x7ff, 0);
}
