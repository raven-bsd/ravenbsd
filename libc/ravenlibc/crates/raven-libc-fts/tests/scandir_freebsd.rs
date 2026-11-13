// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The FreeBSD `scandir_test.c` cases that fit the crate (`fdscandir`/`scandirat` are
//! FreeBSD-only extensions, not in doc 07 P04).
#![cfg(feature = "testutil")]

use raven_libc_abi::generated::errno::EBADF;
use raven_libc_fts::DirEntry;
use raven_libc_fts::fake::FakeFs;
use raven_libc_fts::scandir::{alphasort, scandir};

fn prepare() -> FakeFs {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.mkdir("dir/dir", 0o755);
    fs.file("dir/file", 1);
    fs.symlink("dir/link", "file");
    fs.mkdir("dir/skip", 0o755);
    fs
}

fn verify(entries: &[DirEntry]) {
    assert_eq!(entries.len(), 5);
    let names: Vec<&[u8]> = entries.iter().map(|entry| entry.name.as_slice()).collect();
    assert_eq!(
        names,
        vec![b"link".as_slice(), b"file", b"dir", b"..", b"."]
    );
}

#[test]
fn scandir_test() {
    let mut fs = prepare();
    let mut select = |entry: &DirEntry| entry.name != b"skip";
    let mut compare = |a: &DirEntry, b: &DirEntry| match b.name.cmp(&a.name) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    };
    let entries = scandir(&mut fs, b"dir", Some(&mut select), Some(&mut compare)).unwrap();
    verify(&entries);
}

#[test]
fn scandir_none() {
    let mut fs = prepare();
    let mut select = |_entry: &DirEntry| false;
    let entries = scandir(&mut fs, b"dir", Some(&mut select), Some(&mut alphasort)).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn scandir_error() {
    let mut fs = prepare();
    // opendir is op 1, the first getdents is op 2; the next getdents fails.
    fs.fail_after(3, EBADF);
    let mut select = |_entry: &DirEntry| false;
    let error = scandir(&mut fs, b"dir", Some(&mut select), None).unwrap_err();
    assert_eq!(error.0, EBADF as u16);
}
