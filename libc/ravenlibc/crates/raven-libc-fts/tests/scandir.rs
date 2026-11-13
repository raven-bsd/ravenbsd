// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `scandir`/`alphasort`/`versionsort` and the `DIR` stream on the fake tree.
//!
//! The FreeBSD `scandir_test.c`, `opendir_test.c` and `dir2_test.c` files are absent from the
//! pinned sparse checkout; these cases re-express their contracts (filter, sort, ownership,
//! `ENOTDIR`, `EOVERFLOW`) and the NetBSD `t_dir.c` and OpenBSD `telldir` cases.
#![cfg(feature = "testutil")]

use raven_libc_fts::fake::{FakeFs, NodeKind};
use raven_libc_fts::scandir::{alphasort, scandir, scandir_capped, versionsort};
use raven_libc_fts::{DirStream, DirSys, check_cookie_contract};

fn names(entries: &[raven_libc_fts::DirEntry]) -> Vec<String> {
    entries
        .iter()
        .map(|entry| String::from_utf8_lossy(&entry.name).into_owned())
        .collect()
}

fn dir(names: &[&str]) -> FakeFs {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    for name in names {
        fs.file(format!("d/{name}"), 1);
    }
    fs
}

#[test]
fn scandir_returns_dot_dotdot_and_entries() {
    let mut fs = dir(&["a", "b"]);
    let entries = scandir(&mut fs, b"d", None, None).unwrap();
    assert_eq!(names(&entries), vec![".", "..", "a", "b"]);
}

#[test]
fn scandir_selection_filter() {
    let mut fs = dir(&["a", "b", "c"]);
    let mut select = |entry: &raven_libc_fts::DirEntry| entry.name != b"b";
    let entries = scandir(&mut fs, b"d", Some(&mut select), None).unwrap();
    assert_eq!(names(&entries), vec![".", "..", "a", "c"]);
}

#[test]
fn scandir_alphasort_is_byte_order() {
    let mut fs = dir(&["b", "a", "C"]);
    let entries = scandir(&mut fs, b"d", None, Some(&mut alphasort)).unwrap();
    assert_eq!(names(&entries), vec![".", "..", "C", "a", "b"]);
}

#[test]
fn scandir_versionsort_orders_versions() {
    let mut fs = dir(&["v10", "v2", "v1"]);
    let entries = scandir(&mut fs, b"d", None, Some(&mut versionsort)).unwrap();
    assert_eq!(names(&entries), vec![".", "..", "v1", "v2", "v10"]);
}

#[test]
fn an_inconsistent_comparator_neither_panics_nor_loops() {
    let mut fs = dir(&["a", "b", "c", "d", "e", "f"]);
    let mut counter = 0u32;
    let mut weird = |_a: &raven_libc_fts::DirEntry, _b: &raven_libc_fts::DirEntry| {
        counter = counter.wrapping_mul(1103515245).wrapping_add(12345);
        match counter % 3 {
            0 => -1,
            1 => 0,
            _ => 1,
        }
    };
    let entries = scandir(&mut fs, b"d", None, Some(&mut weird)).unwrap();
    assert_eq!(entries.len(), 8);
}

#[test]
fn the_int_max_cap_is_eoverflow() {
    let mut fs = dir(&["a", "b", "c"]);
    let error = scandir_capped(&mut fs, b"d", None, None, 2).unwrap_err();
    assert_eq!(error.0, raven_libc_abi::generated::errno::EOVERFLOW as u16);
}

#[test]
fn a_non_directory_is_enotdir() {
    let mut fs = FakeFs::new();
    fs.file("f", 1);
    let error = scandir(&mut fs, b"f", None, None).unwrap_err();
    assert_eq!(error.0, raven_libc_abi::generated::errno::ENOTDIR as u16);
}

#[test]
fn scandir_copies_names_of_255_bytes() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    let name = "n".repeat(255);
    fs.file(format!("d/{name}"), 1);
    let entries = scandir(&mut fs, b"d", None, None).unwrap();
    assert!(entries.iter().any(|entry| entry.name.len() == 255));
}

#[test]
fn netbsd_seekdir_basic() {
    let mut fs = FakeFs::new();
    fs.mkdir("t", 0o755);
    fs.file("t/a", 1);
    fs.file("t/b", 1);
    fs.file("t/c", 1);
    let mut dp = DirStream::opendir(&mut fs, b"t").unwrap();
    for _ in 0..2 {
        assert!(dp.readdir(&mut fs).unwrap().is_some());
    }
    let first = dp.readdir(&mut fs).unwrap().unwrap();
    let here = dp.telldir();
    assert_ne!(here, -1);
    assert_eq!(first.name, b"a");
    let wasname = dp.readdir(&mut fs).unwrap().unwrap().name;
    drop(dp.readdir(&mut fs).unwrap().unwrap());
    dp.seekdir(&mut fs, here);
    let entry = dp.readdir(&mut fs).unwrap().unwrap();
    assert_eq!(entry.name, wasname);
    dp.seekdir(&mut fs, here);
    let here2 = dp.telldir();
    let entry = dp.readdir(&mut fs).unwrap().unwrap();
    assert_eq!(entry.name, wasname);
    dp.seekdir(&mut fs, here2);
    let entry = dp.readdir(&mut fs).unwrap().unwrap();
    assert_eq!(entry.name, wasname);
    dp.closedir(&mut fs).unwrap();
}

#[test]
fn openbsd_telldir_survives_an_lseek_on_the_fd() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    for i in 0..8 {
        fs.file(format!("d/f{i}"), 1);
    }
    let mut dp = DirStream::opendir(&mut fs, b"d").unwrap();
    for _ in 0..4 {
        dp.readdir(&mut fs).unwrap();
    }
    let here = dp.telldir();
    // The OpenBSD telldir regress seeks the descriptor itself, then needs telldir/seekdir to
    // still work (a position list would survive; a d_off cookie must be re-issued).
    let fd = dp.dirfd();
    let _ = fs.lseek(fd, 0, raven_libc_fts::Whence::Set);
    dp.seekdir(&mut fs, here);
    let entry = dp.readdir(&mut fs).unwrap().unwrap();
    assert_eq!(entry.name, b"f2");
    dp.closedir(&mut fs).unwrap();
}

#[test]
fn the_cookie_contract_holds_on_a_large_fake_directory() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    for i in 0..70 {
        fs.file(format!("d/f{i}"), 1);
        fs.mkdir(format!("d/s{i}"), 0o755);
    }
    check_cookie_contract(&mut fs, b"d").unwrap();
}

#[test]
fn special_nodes_have_their_dirent_type() {
    let mut fs = FakeFs::new();
    fs.mkdir("d", 0o755);
    fs.node("d/fifo", NodeKind::Fifo);
    fs.node("d/sock", NodeKind::Socket);
    fs.node("d/chr", NodeKind::Char);
    fs.node("d/blk", NodeKind::Block);
    let entries = scandir(&mut fs, b"d", None, None).unwrap();
    let types: Vec<u8> = entries
        .iter()
        .filter(|entry| entry.name != b"." && entry.name != b"..")
        .map(|entry| entry.typ)
        .collect();
    assert_eq!(types, vec![1, 12, 2, 6]);
}
