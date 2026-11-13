// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `ttyent` tests: `/etc/ttys` parsing, quoting, comments, the keyword union and lookup.

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
use crate::testutil::FakeFiles;

const TTYS: &[u8] = b"# /etc/ttys\n\
console\t\"/usr/libexec/getty std.9600\"\tvt100\ton secure\n\
ttyv0\t\"/usr/libexec/getty Pc\"\tcons25\ton secure window=80x24\n\
ttyv1\t\"/usr/libexec/getty Pc\"\tcons25\tonifexists local rtscts softcar mdmbuf\n\
ttyv2\tnone\tdumb\toff insecure dialup network group=tty\n\
ttyv3\t\"/usr/libexec/getty \\\"x\\\"\"\tvt100\ton # the comment\n";

fn open(contents: &[u8]) -> TtyDb {
    let mut files = FakeFiles::new();
    files.insert(b"/etc/ttys", contents);
    TtyDb::open(&mut files, b"/etc/ttys").unwrap()
}

#[test]
fn parses_every_entry() {
    let db = open(TTYS);
    assert_eq!(db.entries.len(), 5);
    let first = &db.entries[0];
    assert_eq!(first.name, b"console");
    assert_eq!(first.getty, b"/usr/libexec/getty std.9600");
    assert_eq!(first.type_, b"vt100");
    assert_eq!(first.status & TTY_ON, TTY_ON);
    assert_eq!(first.status & TTY_SECURE, TTY_SECURE);
}

#[test]
fn window_and_group_values() {
    let db = open(TTYS);
    assert_eq!(db.entries[1].window, b"80x24");
    assert_eq!(db.entries[3].group, b"tty");
    assert_eq!(db.entries[2].group, b"");
}

#[test]
fn openbsd_keywords_map_to_the_new_bits() {
    let db = open(TTYS);
    let entry = &db.entries[2];
    assert_eq!(entry.status & TTY_IFEXISTS, TTY_IFEXISTS);
    assert_eq!(entry.status & TTY_LOCAL, TTY_LOCAL);
    assert_eq!(entry.status & TTY_RTSCTS, TTY_RTSCTS);
    assert_eq!(entry.status & TTY_SOFTCAR, TTY_SOFTCAR);
    assert_eq!(entry.status & TTY_MDMBUF, TTY_MDMBUF);
    assert_eq!(entry.status & TTY_ON, 0);
}

#[test]
fn off_and_insecure_clear_bits() {
    let db = open(TTYS);
    let entry = &db.entries[3];
    assert_eq!(entry.status & TTY_ON, 0);
    assert_eq!(entry.status & TTY_SECURE, 0);
    assert_eq!(entry.status & TTY_DIALUP, TTY_DIALUP);
    assert_eq!(entry.status & TTY_NETWORK, TTY_NETWORK);
}

#[test]
fn quoted_escaped_quote() {
    let db = open(TTYS);
    assert_eq!(db.entries[4].getty, b"/usr/libexec/getty \"x\"");
}

#[test]
fn comments_are_kept() {
    let db = open(TTYS);
    assert_eq!(db.entries[4].comment, b"the comment");
    assert_eq!(db.entries[0].comment, b"");
}

#[test]
fn comment_lines_are_skipped() {
    let db = open(TTYS);
    assert_eq!(db.entries[0].name, b"console");
}

#[test]
fn next_and_rewind() {
    let mut db = open(TTYS);
    assert_eq!(db.next().unwrap().name, b"console");
    assert_eq!(db.next().unwrap().name, b"ttyv0");
    db.rewind();
    assert_eq!(db.next().unwrap().name, b"console");
    for _ in 0..5 {
        let _ = db.next();
    }
    assert!(db.next().is_none());
}

#[test]
fn by_name_finds_the_entry() {
    let db = open(TTYS);
    assert_eq!(db.by_name(b"ttyv1").unwrap().type_, b"cons25");
    assert!(db.by_name(b"nope").is_none());
}

#[test]
fn empty_file_has_no_entries() {
    let db = open(b"");
    assert!(db.is_empty());
}

#[test]
fn missing_file_is_an_error() {
    let mut files = FakeFiles::new();
    assert_eq!(
        TtyDb::open(&mut files, b"/etc/ttys").unwrap_err(),
        Errno(raven_libc_abi::errno::ENOENT as u16)
    );
}

#[test]
fn getty_may_be_absent() {
    let db = open(b"ttyv0\n");
    let entry = &db.entries[0];
    assert_eq!(entry.name, b"ttyv0");
    assert_eq!(entry.getty, b"");
    assert_eq!(entry.type_, b"");
    assert_eq!(entry.status, 0);
}

#[test]
fn the_second_field_is_always_the_getty() {
    // "ttyv0 on" is a getty named "on", not a status keyword (the donor's field order).
    let db = open(b"ttyv0\ton\n");
    let entry = &db.entries[0];
    assert_eq!(entry.getty, b"on");
    assert_eq!(entry.status, 0);
}

#[test]
fn last_line_without_newline() {
    let db = open(b"ttyv0\ton");
    assert_eq!(db.entries[0].name, b"ttyv0");
}
