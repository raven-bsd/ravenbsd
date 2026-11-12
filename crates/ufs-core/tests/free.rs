// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Freeing the last link must leave a filesystem that `check` considers clean.

mod common;

use common::VecDisk;
use ufs_core::check::{Mode, check};
use ufs_core::mkfs::{MkfsOptions, format, format_journaled};

const MIB: u64 = 1024 * 1024;

fn options(size: u64, version: u8) -> MkfsOptions<'static> {
    MkfsOptions {
        size,
        version,
        block_size: 4096,
        fragment_size: 512,
        bytes_per_inode: 4096,
        minfree: 5,
        label: "k55-free",
        timestamp: 1_767_225_600,
    }
}

#[test]
fn unlink_and_rmdir_leave_no_stale_dinodes() {
    for version in [1, 2] {
        let size = 8 * MIB;
        let disk = VecDisk::new(vec![0u8; usize::try_from(size).expect("size")]);
        let mut ufs = format(disk, &options(size, version)).expect("format");
        let root = ufs.root();

        let file = ufs.create(root, b"dead.txt", 0o644, 0, 0).expect("create");
        ufs.write(file, 0, b"payload\n").expect("write");
        ufs.mkdir(root, b"dead-dir", 0o755, 0, 0).expect("mkdir");
        ufs.symlink(root, b"dead-link", b"dead.txt", 0, 0)
            .expect("symlink");

        ufs.unlink(root, b"dead.txt").expect("unlink file");
        ufs.unlink(root, b"dead-link").expect("unlink link");
        ufs.rmdir(root, b"dead-dir").expect("rmdir");
        let disk = ufs.unmount().expect("unmount");

        let mut disk = disk;
        let report = check(&mut disk, Mode::ReadOnly).expect("check");
        assert!(report.clean_after, "UFS{version}: {:?}", report.findings);
    }
}

#[test]
fn journaled_unlink_and_rmdir_commit_clean() {
    let size = 16 * MIB;
    let disk = VecDisk::new(vec![0u8; usize::try_from(size).expect("size")]);
    let mut ufs = format_journaled(disk, &options(size, 2), MIB).expect("format journaled");
    let root = ufs.root();
    let file = ufs.create(root, b"dead.txt", 0o644, 0, 0).expect("create");
    ufs.write(file, 0, b"journaled\n").expect("write");
    ufs.mkdir(root, b"dead-dir", 0o755, 0, 0).expect("mkdir");
    ufs.unlink(root, b"dead.txt").expect("unlink");
    ufs.rmdir(root, b"dead-dir").expect("rmdir");
    ufs.sync().expect("sync");
    let disk = ufs.unmount().expect("unmount");

    let mut disk = disk;
    let report = check(&mut disk, Mode::ReadOnly).expect("check");
    assert!(report.clean_after, "{:?}", report.findings);
}
