// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `mntent` tests: parsing, the `\040` escapes, the option lookup and formatting.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use super::*;

#[test]
fn parses_a_full_line() {
    let entry = parse_line(b"/dev/sda1 / ext4 rw,relatime 0 0\n").unwrap();
    assert_eq!(entry.fsname, b"/dev/sda1");
    assert_eq!(entry.dir, b"/");
    assert_eq!(entry.type_, b"ext4");
    assert_eq!(entry.opts, b"rw,relatime");
    assert_eq!(entry.freq, 0);
    assert_eq!(entry.passno, 0);
}

#[test]
fn optional_numbers() {
    let entry = parse_line(b"a b c d\n").unwrap();
    assert_eq!(entry.freq, 0);
    assert_eq!(entry.passno, 0);
    let entry = parse_line(b"a b c d 1\n").unwrap();
    assert_eq!(entry.freq, 1);
    assert_eq!(entry.passno, 0);
    let entry = parse_line(b"a b c d 0 2\n").unwrap();
    assert_eq!(entry.passno, 2);
}

#[test]
fn comments_and_blank_lines_are_none() {
    assert!(parse_line(b"# comment\n").is_none());
    assert!(parse_line(b"\n").is_none());
    assert!(parse_line(b"   \t \n").is_none());
    assert!(parse_line(b"a b c\n").is_none());
}

#[test]
fn octal_escapes_are_decoded() {
    let entry = parse_line(b"/dev/a\\040b /mnt/x\\040y ext4 rw 0 0\n").unwrap();
    assert_eq!(entry.fsname, b"/dev/a b");
    assert_eq!(entry.dir, b"/mnt/x y");
}

#[test]
fn backslash_and_unknown_escapes() {
    assert_eq!(unescape(b"a\\\\b"), b"a\\b");
    assert_eq!(unescape(b"a\\q"), b"a\\q");
    assert_eq!(unescape(b"a\\0"), b"a\\0");
    assert_eq!(unescape(b"a\\ttab"), b"a\\ttab");
}

#[test]
fn hasmntopt_matches_fields() {
    let opts = b"rw,nosuid,nodev,size=10m";
    assert_eq!(hasmntopt(opts, b"rw"), Some(0));
    assert_eq!(hasmntopt(opts, b"nosuid"), Some(3));
    assert_eq!(hasmntopt(opts, b"nodev"), Some(10));
    assert_eq!(hasmntopt(opts, b"size"), Some(16));
    assert_eq!(hasmntopt(opts, b"siz"), None);
    assert_eq!(hasmntopt(opts, b"dev"), None);
    assert_eq!(hasmntopt(opts, b"noex"), None);
}

#[test]
fn format_round_trips() {
    let entry = MntEnt {
        fsname: b"/dev/sda1".to_vec(),
        dir: b"/".to_vec(),
        type_: b"ext4".to_vec(),
        opts: b"rw".to_vec(),
        freq: 1,
        passno: 2,
    };
    let mut out = Vec::new();
    format(&entry, &mut out);
    assert_eq!(out, b"/dev/sda1\t/\text4\trw\t1\t2\n");
    assert_eq!(parse_line(&out).unwrap(), entry);
}

#[test]
fn negative_numbers_format() {
    let entry = MntEnt {
        freq: -1,
        passno: -20,
        ..MntEnt::default()
    };
    let mut out = Vec::new();
    format(&entry, &mut out);
    assert_eq!(out, b"\t\t\t\t-1\t-20\n");
}

#[test]
fn default_path_is_etc_mtab() {
    assert_eq!(PATH_MOUNTED, b"/etc/mtab");
}
