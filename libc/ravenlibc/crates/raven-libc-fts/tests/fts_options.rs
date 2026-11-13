// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The ten FreeBSD `fts_options_test.c` cases, by name, over the fake tree.
//!
//! The tables are generated data (`tests/data/fts_options.rs`). The C helper compares
//! `fts_info`, `fts_name`, `fts_accpath`, `fts_level` and the order; this port compares the same
//! fields, but `fts_accpath` only for the cases where the donor itself runs with `FTS_NOCHDIR`
//! (LOGICAL, PHYSICAL|NOCHDIR), because H-FTS-1 forces the full path everywhere.
#![cfg(feature = "testutil")]

use raven_libc_fts::fake::FakeFs;
use raven_libc_fts::*;

include!("data/fts_options.rs");

const PATHS: &[&[u8]] = &[b"dir", b"dirl", b"file", b"filel", b"dead", b"noent"];

fn prepare() -> FakeFs {
    let mut fs = FakeFs::new();
    fs.mkdir("dir", 0o755);
    fs.file("dir/file", 0);
    fs.mkdir("dir/sd", 0o750);
    fs.mkdir("dir/sd/sd", 0o700);
    fs.symlink("dir/up", "..");
    fs.symlink("dirl", "dir");
    fs.file("file", 0);
    fs.symlink("filel", "file");
    fs.symlink("dead", "noent");
    fs
}

fn lexical(left: &FtsEnt, right: &FtsEnt) -> i32 {
    match left.name.cmp(&right.name) {
        core::cmp::Ordering::Less => -1,
        core::cmp::Ordering::Equal => 0,
        core::cmp::Ordering::Greater => 1,
    }
}

fn check(name: &str, flags: u32, table: &[(i32, &str, &str)], check_path: bool) {
    let mut fs = prepare();
    let compar: Compar = Box::new(lexical);
    let mut fts = Fts::open(&mut fs, AT_FDCWD, PATHS, flags, Some(compar)).unwrap();
    let mut level: i64 = 0;
    let mut index = 0usize;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        let Some(&(info, expect_name, expect_path)) = table.get(index) else {
            panic!("{name}: extra node {:?} info {}", node.name, node.info);
        };
        if info == FTS_DP || info == FTS_DNR {
            level -= 1;
        }
        assert_eq!(node.info, info, "{name}[{index}]: info");
        assert_eq!(node.name, expect_name.as_bytes(), "{name}[{index}]: name");
        assert_eq!(node.level, level, "{name}[{index}]: level");
        if check_path {
            assert_eq!(node.path, expect_path.as_bytes(), "{name}[{index}]: path");
        }
        if info == FTS_D {
            level += 1;
        }
        index += 1;
    }
    fts.close(&mut fs).unwrap();
    assert_eq!(index, table.len(), "{name}: node count");
}

#[test]
fn fts_options_logical() {
    check("logical", FTS_LOGICAL, FTS_OPTIONS_LOGICAL, true);
}

#[test]
fn fts_options_logical_nostat() {
    // The C file marks this expectation `expect_fail`: FreeBSD's LOGICAL overrides NOSTAT, so
    // the real walk reports D/F, not NSOK. This port matches the implementation, not the
    // hypothetical table; this is the recorded difference.
    let mut fs = prepare();
    let compar: Compar = Box::new(lexical);
    let mut fts = Fts::open(
        &mut fs,
        AT_FDCWD,
        PATHS,
        FTS_LOGICAL | FTS_NOSTAT,
        Some(compar),
    )
    .unwrap();
    let mut saw_file = false;
    while let Some(node) = fts.read(&mut fs).unwrap() {
        assert!(node.statp.is_none());
        if node.name == b"file" {
            assert_eq!(node.info, FTS_F);
            saw_file = true;
        }
    }
    fts.close(&mut fs).unwrap();
    assert!(saw_file);
}

#[test]
fn fts_options_logical_seedot() {
    check(
        "logical_seedot",
        FTS_LOGICAL | FTS_SEEDOT,
        FTS_OPTIONS_LOGICAL_SEEDOT,
        true,
    );
}

#[test]
fn fts_options_physical() {
    check("physical", FTS_PHYSICAL, FTS_OPTIONS_PHYSICAL, false);
}

#[test]
fn fts_options_physical_nochdir() {
    check(
        "physical_nochdir",
        FTS_PHYSICAL | FTS_NOCHDIR,
        FTS_OPTIONS_PHYSICAL_NOCHDIR,
        true,
    );
}

#[test]
fn fts_options_physical_comfollow() {
    check(
        "physical_comfollow",
        FTS_PHYSICAL | FTS_COMFOLLOW,
        FTS_OPTIONS_PHYSICAL_COMFOLLOW,
        false,
    );
}

#[test]
fn fts_options_physical_comfollowdir() {
    check(
        "physical_comfollowdir",
        FTS_PHYSICAL | FTS_COMFOLLOWDIR,
        FTS_OPTIONS_PHYSICAL_COMFOLLOWDIR,
        false,
    );
}

#[test]
fn fts_options_physical_nostat() {
    check(
        "physical_nostat",
        FTS_PHYSICAL | FTS_NOSTAT,
        FTS_OPTIONS_PHYSICAL_NOSTAT,
        false,
    );
}

#[test]
fn fts_options_physical_nostat_type() {
    check(
        "physical_nostat_type",
        FTS_PHYSICAL | FTS_NOSTAT_TYPE,
        FTS_OPTIONS_PHYSICAL_NOSTAT_TYPE,
        false,
    );
}

#[test]
fn fts_options_physical_seedot() {
    check(
        "physical_seedot",
        FTS_PHYSICAL | FTS_SEEDOT,
        FTS_OPTIONS_PHYSICAL_SEEDOT,
        false,
    );
}
