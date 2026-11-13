// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The FreeBSD `fts_set_test.c` cases, by name, over the fake tree.
//!
//! The two pointer cases (`clientptr_roundtrip`, `get_stream_backpointer`) are the C shell's
//! `FTS*`/`FTSENT*` mirror, not this pure crate; they are recorded as not ported in the report.
#![cfg(feature = "testutil")]

use raven_libc_fts::fake::FakeFs;
use raven_libc_fts::*;

#[test]
fn invalid_options() {
    let mut fs = FakeFs::new();
    fs.file("f", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"f"], FTS_PHYSICAL, None).unwrap();
    let root = fts.read(&mut fs).unwrap().unwrap().id;
    assert_eq!(
        fts.set(root, 99).err().map(|error| error.0),
        Some(raven_libc_abi::generated::errno::EINVAL as u16)
    );
    fts.close(&mut fs).unwrap();
}

#[test]
fn again() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/file", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    let mut revisit = 0;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        if node.info == FTS_F && revisit == 0 {
            let id = node.id;
            assert!(fts.set(id, FTS_AGAIN).is_ok());
            revisit = 1;
        } else if node.info == FTS_F && revisit >= 1 {
            revisit += 1;
        }
    }
    fts.close(&mut fs).unwrap();
    assert_eq!(revisit, 2);
}

#[test]
fn again_consecutive() {
    let mut fs = FakeFs::new();
    fs.file("file", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"file"], FTS_PHYSICAL, None).unwrap();
    let mut visits = 0;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        if node.info == FTS_F {
            visits += 1;
            if visits < 3 {
                let id = node.id;
                assert!(fts.set(id, FTS_AGAIN).is_ok());
            }
        }
    }
    fts.close(&mut fs).unwrap();
    assert_eq!(visits, 3);
}

#[test]
fn follow_symlink_to_file() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/target", 0);
    fs.symlink("dir/link", "target");
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    let mut followed = false;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        if node.info == FTS_SL && node.name == b"link" {
            let id = node.id;
            assert!(fts.set(id, FTS_FOLLOW).is_ok());
        } else if node.info == FTS_F && node.name == b"link" {
            followed = true;
        }
    }
    fts.close(&mut fs).unwrap();
    assert!(followed);
}

#[test]
fn follow_symlink_to_dir() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.mkdir("dir/real", 0o755);
    fs.file("dir/real/inside", 0);
    fs.symlink("dir/link", "real");
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    let mut saw_inside = false;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        let id = node.id;
        let info = node.info;
        let name = node.name.clone();
        let path = node.path.clone();
        if info == FTS_SL && name == b"link" {
            assert!(fts.set(id, FTS_FOLLOW).is_ok());
        }
        if info == FTS_F && name == b"inside" && path == b"dir/link/inside" {
            saw_inside = true;
        }
    }
    fts.close(&mut fs).unwrap();
    assert!(saw_inside);
}

#[test]
fn follow_dead_symlink() {
    let mut fs = FakeFs::new();
    fs.symlink("dead", "no-such-target");
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dead"], FTS_LOGICAL, None).unwrap();
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.info, FTS_SLNONE);
    let id = node.id;
    assert!(fts.set(id, FTS_FOLLOW).is_ok());
    let node = fts.read(&mut fs).unwrap().unwrap();
    assert_eq!(node.info, FTS_SLNONE);
    fts.close(&mut fs).unwrap();
}

#[test]
fn skip() {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.mkdir("dir/skip_me", 0o755);
    fs.file("dir/skip_me/inside", 0);
    fs.file("dir/sibling", 0);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, &[b"dir"], FTS_PHYSICAL, None).unwrap();
    let mut saw_inside = false;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        let id = node.id;
        let info = node.info;
        let name = node.name.clone();
        if info == FTS_D && name == b"skip_me" {
            assert!(fts.set(id, FTS_SKIP).is_ok());
        }
        if name == b"inside" {
            saw_inside = true;
        }
    }
    fts.close(&mut fs).unwrap();
    assert!(!saw_inside);
}
