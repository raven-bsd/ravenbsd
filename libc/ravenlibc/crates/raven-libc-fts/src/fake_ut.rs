// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the fake filesystem of doc 07 4.5.
// Test-only code is exempt from the production lints (17-crate-layout 9.3).
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use alloc::boxed::Box;
use alloc::vec::Vec;

use raven_libc_abi::generated::errno::{EACCES, ELOOP, EMFILE, ENOENT, ENOTDIR};
use raven_libc_abi::generated::fcntl::{O_DIRECTORY, O_NOFOLLOW, O_RDONLY};

use super::*;
use crate::dirent::DirStream;
use crate::{Fd, Whence};

#[test]
fn stable_inodes_devices_and_hard_links() {
    let mut fs = FakeFs::new();
    fs.file("a", 10);
    fs.link("b", "a");
    let a = fs.fstatat(crate::AT_FDCWD, b"a", true).unwrap();
    let b = fs.fstatat(crate::AT_FDCWD, b"b", true).unwrap();
    assert_eq!(a.st_ino, b.st_ino);
    assert_eq!(a.st_nlink, 2);
    let a2 = fs.fstatat(crate::AT_FDCWD, b"a", true).unwrap();
    assert_eq!(a.st_ino, a2.st_ino);
    assert_eq!(a.st_dev, a2.st_dev);
}

#[test]
fn mount_changes_the_subtree_device_and_dotdot_returns() {
    let mut fs = FakeFs::new();
    fs.mkdir("mnt", 0o755);
    fs.mkdir("mnt/sub", 0o755);
    fs.file("mnt/sub/f", 1);
    fs.mount("mnt", 7);
    let inner = fs.fstatat(crate::AT_FDCWD, b"mnt/sub/f", true).unwrap();
    assert_eq!(inner.st_dev, 7);
    let parent = fs.fstatat(crate::AT_FDCWD, b"mnt/..", true).unwrap();
    assert_eq!(parent.st_dev, 1);
}

#[test]
fn symlinks_are_followed_and_o_nofollow_reports_eloop() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.file("d/x", 1);
    fs.symlink("l", "d");
    assert!(fs.fstatat(crate::AT_FDCWD, b"l/x", false).is_ok());
    assert_eq!(
        fs.openat(crate::AT_FDCWD, b"l", O_RDONLY as u32 | O_NOFOLLOW as u32)
            .unwrap_err()
            .0,
        ELOOP as u16
    );
}

#[test]
fn a_symlink_loop_is_eloop() {
    let mut fs = FakeFs::new();
    fs.symlink("a", "b");
    fs.symlink("b", "a");
    assert_eq!(
        fs.fstatat(crate::AT_FDCWD, b"a", false).unwrap_err().0,
        ELOOP as u16
    );
}

#[test]
fn o_directory_on_a_file_is_enotdir() {
    let mut fs = FakeFs::new();
    fs.file("f", 1);
    assert_eq!(
        fs.openat(crate::AT_FDCWD, b"f", O_DIRECTORY as u32)
            .unwrap_err()
            .0,
        ENOTDIR as u16
    );
}

#[test]
fn permission_bits_are_honoured() {
    let mut fs = FakeFs::new();
    fs.mkdir("locked", 0o000);
    fs.mkdir("noexec", 0o600);
    assert_eq!(
        fs.openat(crate::AT_FDCWD, b"locked", O_DIRECTORY as u32)
            .unwrap_err()
            .0,
        EACCES as u16
    );
    fs.file("noexec/f", 1);
    assert_eq!(
        fs.fstatat(crate::AT_FDCWD, b"noexec/f", true)
            .unwrap_err()
            .0,
        EACCES as u16
    );
}

#[test]
fn emfile_after_limits_simultaneous_descriptors() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.emfile_after(2);
    let first = fs
        .openat(crate::AT_FDCWD, b"d", O_DIRECTORY as u32)
        .unwrap();
    let second = fs
        .openat(crate::AT_FDCWD, b"d", O_DIRECTORY as u32)
        .unwrap();
    assert_eq!(
        fs.openat(crate::AT_FDCWD, b"d", O_DIRECTORY as u32)
            .unwrap_err()
            .0,
        EMFILE as u16
    );
    fs.close(first).unwrap();
    assert!(fs.openat(crate::AT_FDCWD, b"d", O_DIRECTORY as u32).is_ok());
    fs.close(second).unwrap();
}

#[test]
fn close_reuses_the_lowest_free_descriptor() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    let first = fs.openat(crate::AT_FDCWD, b"d", 0).unwrap();
    let second = fs.openat(crate::AT_FDCWD, b"d", 0).unwrap();
    fs.close(first).unwrap();
    let reused = fs.openat(crate::AT_FDCWD, b"d", 0).unwrap();
    assert_eq!(first, reused);
    assert_ne!(second, reused);
}

#[test]
fn fail_after_fails_exactly_one_call() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    let next = fs.op_count() + 1;
    fs.fail_after(next, ENOENT);
    assert_eq!(
        fs.openat(crate::AT_FDCWD, b"d", 0).unwrap_err().0,
        ENOENT as u16
    );
    assert!(fs.openat(crate::AT_FDCWD, b"d", 0).is_ok());
}

#[test]
fn hook_after_can_swap_a_directory_for_a_symlink() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.file("d/x", 1);
    fs.file("target", 1);
    let after = fs.op_count() + 1;
    fs.hook_after(
        after,
        Box::new(|fs: &mut FakeFs| {
            fs.remove("d");
            fs.symlink("d", "target");
        }),
    );
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    let _ = dir.readdir(&mut fs).unwrap();
    let _ = dir.readdir(&mut fs).unwrap();
    assert_eq!(
        fs.fstatat(crate::AT_FDCWD, b"d", false).unwrap().st_mode
            & raven_libc_abi::generated::stat::S_IFMT,
        raven_libc_abi::generated::stat::S_IFREG
    );
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn getdents_records_are_aligned_and_cookies_resume() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.file("d/abc", 1);
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    let mut seen = Vec::new();
    while let Some(entry) = dir.readdir(&mut fs).unwrap() {
        assert_eq!(entry.reclen as usize % 8, 0);
        assert_eq!(entry.off, seen.len() as i64 + 1);
        seen.push(entry.name);
    }
    assert_eq!(
        seen,
        alloc::vec![b".".to_vec(), b"..".to_vec(), b"abc".to_vec()]
    );
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn lseek_accepts_only_valid_cookies_on_directories() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.file("d/a", 1);
    let fd = fs.openat(crate::AT_FDCWD, b"d", 0).unwrap();
    assert!(fs.lseek(fd, 0, Whence::Set).is_ok());
    assert!(fs.lseek(fd, 3, Whence::Set).is_ok());
    assert!(fs.lseek(fd, 4, Whence::Set).is_err());
    assert!(fs.lseek(fd, -1, Whence::Set).is_err());
    fs.close(fd).unwrap();
}

#[test]
fn the_call_log_records_names_and_flags() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    let fd = fs
        .openat(
            crate::AT_FDCWD,
            b"d",
            O_DIRECTORY as u32 | O_NOFOLLOW as u32,
        )
        .unwrap();
    fs.fstat(fd).unwrap();
    fs.close(fd).unwrap();
    assert_eq!(fs.log.len(), 3);
    assert_eq!(fs.log[0].method, "openat");
    assert_eq!(fs.log[0].name, b"d");
    assert_eq!(fs.log[0].flags & O_NOFOLLOW as u32, O_NOFOLLOW as u32);
    assert_eq!(fs.log[1].method, "fstat");
    assert_eq!(fs.log[2].method, "close");
}

#[test]
fn cwd_is_a_settable_field() {
    let mut fs = FakeFs::new();
    fs.mkdir("a", 0o755);
    fs.file("a/f", 1);
    fs.set_cwd("a");
    assert!(fs.fstatat(crate::AT_FDCWD, b"f", true).is_ok());
    let fd: Fd = fs
        .openat(crate::AT_FDCWD, b".", O_DIRECTORY as u32)
        .unwrap();
    fs.close(fd).unwrap();
}
