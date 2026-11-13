// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The FreeBSD `fts_children_test.c` cases, by name, over the fake tree.
#![cfg(feature = "testutil")]

use raven_libc_fts::fake::FakeFs;
use raven_libc_fts::*;

fn lexical(left: &FtsEnt, right: &FtsEnt) -> i32 {
    match left.name.cmp(&right.name) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

#[test]
fn before_read() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/a", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    let children = fts.children(&mut fs, 0).unwrap();
    assert_eq!(children.len(), 1);
    assert_eq!(children[0].info, FTS_D);
    fts.close(&mut fs).unwrap();
}

#[test]
fn empty_dir() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    assert_eq!(fts.read(&mut fs).unwrap().unwrap().info, FTS_D);
    assert!(fts.children(&mut fs, 0).unwrap().is_empty());
    fts.close(&mut fs).unwrap();
}

#[test]
fn nonempty_dir() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    for name in ["a", "b", "c"] {
        fs.file(format!("dir/{name}"), 0);
    }
    let compar: Compar = Box::new(lexical);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, Some(compar)).unwrap();
    assert_eq!(fts.read(&mut fs).unwrap().unwrap().info, FTS_D);
    let children = fts.children(&mut fs, 0).unwrap();
    let names: Vec<&[u8]> = children.iter().map(|node| node.name.as_slice()).collect();
    assert_eq!(names, vec![b"a".as_slice(), b"b", b"c"]);
    assert!(children.iter().all(|node| node.info == FTS_F));
    fts.close(&mut fs).unwrap();
}

#[test]
fn called_twice() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/x", 0);
    fs.file("dir/y", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    assert_eq!(fts.read(&mut fs).unwrap().unwrap().info, FTS_D);
    let first = fts.children(&mut fs, 0).unwrap().len();
    let second = fts.children(&mut fs, 0).unwrap().len();
    assert_eq!(first, second);
    assert_eq!(second, 2);
    fts.close(&mut fs).unwrap();
}

#[test]
fn nameonly() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/f1", 0);
    fs.file("dir/f2", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    assert_eq!(fts.read(&mut fs).unwrap().unwrap().info, FTS_D);
    let children = fts.children(&mut fs, FTS_NAMEONLY as i32).unwrap();
    assert_eq!(children.len(), 2);
    for node in &children {
        assert!(!node.name.is_empty());
        assert_eq!(node.info, FTS_NSOK);
    }
    while fts.read(&mut fs).unwrap().is_some() {}
    fts.close(&mut fs).unwrap();
}

#[test]
fn nondirectory() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/file", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    assert_eq!(fts.read(&mut fs).unwrap().unwrap().info, FTS_D);
    assert_eq!(fts.read(&mut fs).unwrap().unwrap().info, FTS_F);
    assert!(fts.children(&mut fs, 0).unwrap().is_empty());
    fts.close(&mut fs).unwrap();
}

#[test]
fn invalid_options() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    assert_eq!(
        fts.children(&mut fs, 99).err().map(|error| error.0),
        Some(raven_libc_abi::generated::errno::EINVAL as u16)
    );
    fts.close(&mut fs).unwrap();
}
