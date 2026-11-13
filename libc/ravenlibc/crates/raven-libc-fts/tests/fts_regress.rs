// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The FreeBSD `fts_regress_test.c` cases, by name, over the fake tree.
//!
//! The three time-bounded thread races are made deterministic with the fake's `hook_after`
//! (a tree mutation between two calls) instead of a background thread.
#![cfg(feature = "testutil")]

use raven_libc_fts::fake::FakeFs;
use raven_libc_fts::*;

#[test]
fn read_no_exec_dir() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/file", 0);
    fs.chmod("dir", 0o400);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    let mut saw_d = false;
    let mut saw_file = false;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        if node.info == FTS_D && node.name == b"dir" {
            saw_d = true;
        }
        if node.name == b"file" {
            saw_file = true;
        }
    }
    fts.close(&mut fs).unwrap();
    assert!(saw_d);
    assert!(saw_file);
}

#[test]
fn no_slnone_for_nonsymlink() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/victim", 0);
    // Between the getdents and the stat, remove the file: a failed lstat on a non-symlink is
    // FTS_NS, never FTS_SLNONE.
    let after = fs.op_count() + 4;
    fs.hook_after(after, Box::new(|fs: &mut FakeFs| fs.remove("dir/victim")));
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_LOGICAL, None).unwrap();
    let mut saw_slnone = false;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        if node.info == FTS_SLNONE && node.name == b"victim" {
            saw_slnone = true;
        }
    }
    fts.close(&mut fs).unwrap();
    assert!(!saw_slnone);
}

#[test]
fn readdir_error_detected() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/file", 0);
    fs.chmod("dir", 0o100);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    assert_eq!(fts.read(&mut fs).unwrap().unwrap().info, FTS_D);
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.info, FTS_DNR);
    assert_ne!(node.errno, 0);
    assert!(fts.read(&mut fs).unwrap().is_none());
    fts.close(&mut fs).unwrap();
}

#[test]
fn odirectory_changedir() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.mkdir("dir/sub", 0o755);
    fs.file("dir/sub/file", 0);
    let after = fs.op_count() + 5;
    fs.hook_after(
        after,
        Box::new(|fs: &mut FakeFs| {
            fs.remove("dir/sub");
            fs.file("dir/sub", 0);
        }),
    );
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    let mut count = 0;
    let mut descended = false;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        count += 1;
        if node.path == b"dir/sub/file" {
            descended = true;
        }
    }
    fts.close(&mut fs).unwrap();
    assert!(count < 100);
    assert!(!descended);
}

#[test]
fn concurrent_modification() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/stable", 0);
    let after = fs.op_count() + 3;
    fs.hook_after(
        after,
        Box::new(|fs: &mut FakeFs| {
            fs.file("dir/victim", 0);
            fs.remove("dir/victim");
        }),
    );
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    let mut count = 0;
    while fts.read(&mut fs).unwrap().is_some() {
        count += 1;
    }
    fts.close(&mut fs).unwrap();
    assert!(count >= 3);
}

#[test]
fn accpath_correct_after_descent() {
    let mut fs = FakeFs::new();
    fs.mkdir("a", 0o755);
    fs.mkdir("a/1", 0o755);
    fs.mkdir("a/2", 0o755);
    fs.mkdir("b", 0o755);
    fs.file("a/1/file", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"."], FTS_PHYSICAL, None).unwrap();
    let mut saw_deep = false;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        let info = node.info;
        let name = node.name.clone();
        let path = node.path.clone();
        if info == FTS_D {
            let _ = fts.children(&mut fs, 0).unwrap();
        }
        if name == b"file" && info == FTS_F && path == b"./a/1/file" {
            saw_deep = true;
        }
    }
    fts.close(&mut fs).unwrap();
    assert!(saw_deep);
}

#[test]
fn trailing_slash_children() {
    let mut fs = FakeFs::new();
    fs.mkdir("root", 0o755);
    for index in 1..=3 {
        fs.mkdir(format!("root/sub{index}"), 0o755);
        fs.file(format!("root/sub{index}/file{index}"), 0);
    }
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"root/"], FTS_PHYSICAL, None).unwrap();
    let mut files = 0;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        let info = node.info;
        if info == FTS_D {
            let _ = fts.children(&mut fs, 0).unwrap();
        }
        assert_ne!(info, FTS_DNR);
        assert_ne!(info, FTS_ERR);
        if info == FTS_F {
            files += 1;
        }
    }
    fts.close(&mut fs).unwrap();
    assert_eq!(files, 3);
}
