// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The FreeBSD `opendir_test.c` cases, by name, over the fake tree.
#![cfg(feature = "testutil")]

use raven_libc_abi::generated::errno::{EBADF, ENOTDIR};
use raven_libc_fts::fake::{FakeFs, NodeKind};
use raven_libc_fts::*;

fn prepare() -> FakeFs {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.mkdir("dir/subdir", 0o755);
    fs
}

fn check(fs: &mut FakeFs, dir: &mut DirStream) {
    let first = dir.readdir(fs).unwrap().unwrap();
    assert_eq!(first.name, b".");
    assert_eq!(first.typ, 4);
    let second = dir.readdir(fs).unwrap().unwrap();
    assert_eq!(second.name, b"..");
    assert_eq!(second.typ, 4);
    let third = dir.readdir(fs).unwrap().unwrap();
    assert_eq!(third.name, b"subdir");
    assert_eq!(third.typ, 4);
    assert!(dir.readdir(fs).unwrap().is_none());
    assert!(dir.readdir(fs).unwrap().is_none());
}

#[test]
fn opendir_ok() {
    let mut fs = prepare();
    let mut dir = DirStream::opendir(&mut fs, b"dir").unwrap();
    check(&mut fs, &mut dir);
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn opendir_fifo() {
    let mut fs = FakeFs::new();
    fs.node("fifo", NodeKind::Fifo);
    assert_eq!(
        DirStream::opendir(&mut fs, b"fifo")
            .err()
            .map(|error| error.0),
        Some(ENOTDIR as u16)
    );
}

#[test]
fn fdopendir_ok() {
    let mut fs = prepare();
    let fd = fs
        .openat(
            AT_FDCWD,
            b"dir",
            raven_libc_abi::generated::fcntl::O_DIRECTORY as u32,
        )
        .unwrap();
    let mut dir = DirStream::fdopendir(&mut fs, fd).unwrap();
    assert_eq!(dir.dirfd(), fd);
    check(&mut fs, &mut dir);
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn fdopendir_ebadf() {
    let mut fs = prepare();
    let fd = fs.openat(AT_FDCWD, b"dir", 0).unwrap();
    fs.close(fd).unwrap();
    assert_eq!(
        DirStream::fdopendir(&mut fs, fd).err().map(|error| error.0),
        Some(EBADF as u16)
    );
}

#[test]
fn fdopendir_enotdir() {
    let mut fs = FakeFs::new();
    fs.file("file", 1);
    let fd = fs.openat(AT_FDCWD, b"file", 0).unwrap();
    assert_eq!(
        DirStream::fdopendir(&mut fs, fd).err().map(|error| error.0),
        Some(ENOTDIR as u16)
    );
    fs.close(fd).unwrap();
}
