// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The FreeBSD `dir2_test.c` cases, by name, over the fake tree.
#![cfg(feature = "testutil")]

use raven_libc_fts::fake::FakeFs;
use raven_libc_fts::*;

#[test]
fn telldir_after_seekdir() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    for i in 0..1000 {
        fs.file(format!("d/{i}"), 0);
    }
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    let beginning = dir.telldir();
    for _ in 0..500 {
        assert!(dir.readdir(&mut fs).unwrap().is_some());
    }
    let middle = dir.telldir();
    for _ in 500..999 {
        assert!(dir.readdir(&mut fs).unwrap().is_some());
    }
    let end = dir.telldir();
    for bookmark in [beginning, middle, end] {
        dir.seekdir(&mut fs, bookmark);
        assert_eq!(dir.telldir(), bookmark);
        assert!(dir.readdir(&mut fs).unwrap().is_some());
    }
    dir.closedir(&mut fs).unwrap();
}

#[test]
fn telldir_at_end_of_block() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    for i in 0..127 {
        fs.file(format!("d/{i}"), 0);
    }
    fs.set_dirent_buf_limit(512);
    let mut dir = DirStream::opendir(&mut fs, b"d").unwrap();
    for _ in 0..128 {
        assert!(dir.readdir(&mut fs).unwrap().is_some());
    }
    let td = dir.telldir();
    let last = dir.readdir(&mut fs).unwrap().unwrap().name;
    dir.seekdir(&mut fs, td);
    assert_eq!(dir.readdir(&mut fs).unwrap().unwrap().name, last);
    dir.closedir(&mut fs).unwrap();
}
