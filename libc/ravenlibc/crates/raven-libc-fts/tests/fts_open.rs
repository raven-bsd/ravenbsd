// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The FreeBSD `fts_open_test.c` cases, by name, over the fake tree.
#![cfg(feature = "testutil")]

use raven_libc_abi::generated::errno::{EACCES, EINVAL, ENOENT};
use raven_libc_fts::fake::FakeFs;
use raven_libc_fts::*;

fn walk(fs: &mut FakeFs, roots: &[&[u8]], options: u32) -> Vec<(i32, Vec<u8>)> {
    let mut fts = Fts::open(fs, AT_FDCWD, roots, options, None).unwrap();
    let mut out = Vec::new();
    while let Some(node) = fts.read(fs).unwrap() {
        out.push((node.info, node.path.clone()));
    }
    fts.close(fs).unwrap();
    out
}

#[test]
fn invalid_options() {
    let mut fs = FakeFs::new();
    assert_eq!(
        Fts::open(&mut fs, AT_FDCWD, &[b"."], 0x1_0000, None)
            .err()
            .map(|error| error.0),
        Some(EINVAL as u16)
    );
}

#[test]
fn empty_argv() {
    let mut fs = FakeFs::new();
    assert_eq!(
        Fts::open(&mut fs, AT_FDCWD, &[], FTS_PHYSICAL, None)
            .err()
            .map(|error| error.0),
        Some(EINVAL as u16)
    );
}

#[test]
fn empty_path_string() {
    let mut fs = FakeFs::new();
    let nodes = walk(&mut fs, &[b""], FTS_PHYSICAL);
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].0, FTS_NS);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b""], FTS_PHYSICAL, None).unwrap();
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.info, FTS_NS);
    assert_eq!(node.errno, ENOENT);
    fts.close(&mut fs).unwrap();
}

#[test]
fn nonexistent_path() {
    let mut fs = FakeFs::new();
    let mut fts = Fts::open(
        &mut fs,
        AT_FDCWD,
        &[b"this-path-does-not-exist"],
        FTS_PHYSICAL,
        None,
    )
    .unwrap();
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.info, FTS_NS);
    assert_eq!(node.errno, ENOENT);
    assert!(fts.read(&mut fs).unwrap().is_none());
    fts.close(&mut fs).unwrap();
}

#[test]
fn trailing_slash() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/file", 0);
    let nodes = walk(&mut fs, &[b"dir/"], FTS_PHYSICAL);
    assert!(nodes.iter().any(|(info, _)| *info == FTS_D));
    assert!(nodes.iter().any(|(info, _)| *info == FTS_F));
    assert!(nodes.iter().any(|(info, _)| *info == FTS_DP));
}

#[test]
fn unreadable_dir() {
    let mut fs = FakeFs::new();
    fs.mkdir("unr", 0o000);
    let nodes = walk(&mut fs, &[b"unr"], FTS_PHYSICAL);
    assert_eq!(nodes[0].0, FTS_D);
    assert_eq!(nodes[1].0, FTS_DNR);
    assert!(!nodes.iter().any(|(info, _)| *info == FTS_DP));
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"unr"], FTS_PHYSICAL, None).unwrap();
    let mut errno = 0;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        if node.info == FTS_DNR {
            errno = node.errno;
        }
    }
    fts.close(&mut fs).unwrap();
    assert_eq!(errno, EACCES);
}

#[test]
fn multiple_roots() {
    let mut fs = FakeFs::new();
    fs.mkdir("a", 0o755);
    fs.mkdir("b", 0o755);
    fs.file("a/x", 0);
    fs.file("b/y", 0);
    let nodes = walk(&mut fs, &[b"a", b"b"], FTS_PHYSICAL);
    let sequence: Vec<(i32, Vec<u8>)> = nodes;
    assert_eq!(sequence[0], (FTS_D, b"a".to_vec()));
    assert_eq!(sequence[1], (FTS_F, b"a/x".to_vec()));
    assert_eq!(sequence[2], (FTS_DP, b"a".to_vec()));
    assert_eq!(sequence[3], (FTS_D, b"b".to_vec()));
    assert_eq!(sequence[4], (FTS_F, b"b/y".to_vec()));
    assert_eq!(sequence[5], (FTS_DP, b"b".to_vec()));
}
