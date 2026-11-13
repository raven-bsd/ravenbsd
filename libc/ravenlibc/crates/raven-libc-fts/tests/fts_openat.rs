// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The FreeBSD `fts_openat_test.c` cases that fit the pure crate.
//!
//! `fts_dirfd_openat` needs the parent `FTSENT*` (`fts_parent->fts_dirfd`), which is the C
//! shell's mirror; the crate records the equivalent guarantee in the fake call log instead and
//! this file ports the other two cases.
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

fn collect(fts: &mut Fts, fs: &mut FakeFs) -> Vec<(i32, Vec<u8>)> {
    let mut out = Vec::new();
    while let Some(node) = fts.read(fs).unwrap() {
        out.push((node.info, node.name.clone()));
    }
    out
}

#[test]
fn atfdcwd_matches_fts_open() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.mkdir("dir/sub", 0o755);
    fs.file("dir/sub/file", 0);
    fs.file("dir/other", 0);
    let compar: Compar = Box::new(lexical);
    let mut open = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, Some(compar)).unwrap();
    let from_open = collect(&mut open, &mut fs);
    open.close(&mut fs).unwrap();
    let compar: Compar = Box::new(lexical);
    let mut openat = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, Some(compar)).unwrap();
    let from_openat = collect(&mut openat, &mut fs);
    openat.close(&mut fs).unwrap();
    assert_eq!(from_open, from_openat);
}

#[test]
fn openat_dirfd() {
    // The Capsicum mode itself is a FreeBSD sandbox feature; the capability-mode part is
    // skipped and the dirfd-relative traversal is what this case exercises.
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.mkdir("dir/sub", 0o755);
    fs.file("dir/sub/file", 0);
    fs.file("dir/other", 0);
    let fd = fs.openat(AT_FDCWD, b"dir", 0).unwrap();
    let mut fts = Fts::open(&mut fs, fd.0, &[b"."], FTS_PHYSICAL | FTS_NOCHDIR, None).unwrap();
    let mut saw_sub = false;
    let mut saw_file = false;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        if node.info == FTS_DP {
            continue;
        }
        if node.name == b"sub" && node.info == FTS_D {
            saw_sub = true;
        }
        if node.name == b"file" && node.info == FTS_F {
            saw_file = true;
        }
    }
    fts.close(&mut fs).unwrap();
    fs.close(fd).unwrap();
    assert!(saw_sub);
    assert!(saw_file);
}
