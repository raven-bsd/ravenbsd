// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `getcap` tests: the `getcaptest.c` cases, `shortcap.in`, `tc=` chains, escapes, numbers
//! and the proof that no `.db` file is ever opened.

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

const CAPS: &[u8] = br"# a comment
short|shortcut:\
	:am=short:tc=long:
long:\
	:al=long:co#42:hex#0x1f:oct#010:str=value\072with\Eescapes:
missing:\
	:tc=nosuch:am=yes:
loop1:\
	:tc=loop2:
loop2:\
	:tc=loop1:
";

fn db() -> (CapDb, FakeFiles) {
    let mut files = FakeFiles::new();
    files.insert(b"/etc/termcap", CAPS);
    (CapDb::new(), files)
}

#[test]
fn cgetmatch_matches_names() {
    assert!(matches(b"short|shortcut:am=short:", b"short"));
    assert!(matches(b"short|shortcut:am=short:", b"shortcut"));
    assert!(!matches(b"short|shortcut:am=short:", b"shortcu"));
    assert!(!matches(b"short|shortcut:am=short:", b"long"));
    assert!(!matches(b"short|shortcut:am=short:", b""));
}

#[test]
fn cgetent_finds_and_expands_tc() {
    let (mut db, mut files) = db();
    let record = db.ent(&mut files, &[b"/etc/termcap"], b"short").unwrap();
    assert!(matches(&record, b"short"));
    // The tc=long record lost its name field and gained its body.
    assert!(record.windows(4).any(|w| w == b"al=l"));
    assert!(record.windows(4).any(|w| w == b"am=s"));
    assert!(!record.windows(6).any(|w| w == b"tc=lon"));
}

#[test]
fn cgetent_not_found() {
    let (mut db, mut files) = db();
    assert_eq!(
        db.ent(&mut files, &[b"/etc/termcap"], b"nosuch"),
        Err(CgetError::NotFound)
    );
}

#[test]
fn missing_file_is_a_system_error() {
    let (mut db, mut files) = db();
    assert_eq!(
        db.ent(&mut files, &[b"/no/such/file"], b"short"),
        Err(CgetError::Errno(Errno(
            raven_libc_abi::errno::ENOENT as u16
        )))
    );
}

#[test]
fn unresolved_tc_is_kept_in_the_record() {
    let (mut db, mut files) = db();
    let record = db.ent(&mut files, &[b"/etc/termcap"], b"missing").unwrap();
    assert!(record.windows(10).any(|w| w == b"tc=nosuch:"));
}

#[test]
fn a_tc_loop_is_detected() {
    let (mut db, mut files) = db();
    assert_eq!(
        db.ent(&mut files, &[b"/etc/termcap"], b"loop1"),
        Err(CgetError::Loop)
    );
}

#[test]
fn deep_tc_chains_use_the_donor_limit() {
    let mut files = FakeFiles::new();
    let mut text = alloc::vec::Vec::new();
    // r0 -> ... -> r31 is a chain of 32 records: the deepest level is 31, accepted.
    for level in 0..32u32 {
        if level == 31 {
            text.extend_from_slice(alloc::format!("r{level}:am=yes:\n").as_bytes());
        } else {
            text.extend_from_slice(alloc::format!("r{level}:tc=r{}:\n", level + 1).as_bytes());
        }
    }
    // q100 -> ... -> q134 is 35 levels: from q100 the depth reaches 34 > 32.
    for level in 100..135u32 {
        if level == 134 {
            text.extend_from_slice(alloc::format!("q{level}:am=yes:\n").as_bytes());
        } else {
            text.extend_from_slice(alloc::format!("q{level}:tc=q{}:\n", level + 1).as_bytes());
        }
    }
    files.insert(b"/etc/caps", &text);
    let mut db = CapDb::new();
    assert!(db.ent(&mut files, &[b"/etc/caps"], b"r0").is_ok());
    assert_eq!(
        db.ent(&mut files, &[b"/etc/caps"], b"q100"),
        Err(CgetError::Loop)
    );
    // Exactly 32 tc hops from q102 succeed.
    assert!(db.ent(&mut files, &[b"/etc/caps"], b"q102").is_ok());
    assert_eq!(
        db.ent(&mut files, &[b"/etc/caps"], b"q101"),
        Err(CgetError::Loop)
    );
}

#[test]
fn cgetcap_finds_values() {
    let record = b"long:al=long:co#42:str=value:";
    assert_eq!(cap(record, b"al", b'='), Some(8));
    assert_eq!(&record[8..12], b"long");
    assert_eq!(cap(record, b"co", b'#'), Some(16));
    assert_eq!(cap(record, b"str", b'='), Some(23));
    assert_eq!(cap(record, b"missing", b'='), None);
    assert_eq!(cap(record, b"al", b':'), None);
    // The `@` cancels a capability.
    assert_eq!(cap(b"x:al@:al=1:", b"al", b'='), None);
}

#[test]
fn cgetnum_bases() {
    let record = b"x:n1#42:n2#0x1f:n3#010:n4#7:n5#:";
    assert_eq!(num(record, b"n1"), Ok(42));
    assert_eq!(num(record, b"n2"), Ok(31));
    assert_eq!(num(record, b"n3"), Ok(8));
    assert_eq!(num(record, b"n4"), Ok(7));
    assert_eq!(num(record, b"n5"), Ok(0));
    assert_eq!(num(record, b"n6"), Err(CgetError::NotFound));
}

#[test]
fn cgetstr_decodes_escapes() {
    let record = b"x:e1=a\\072b:e2=^A:e3=\\E:e4=\\n:e5=\\q:e6=\\101:";
    assert_eq!(str_(record, b"e1").unwrap(), b"a:b");
    assert_eq!(str_(record, b"e2").unwrap(), b"\x01");
    assert_eq!(str_(record, b"e3").unwrap(), b"\x1b");
    assert_eq!(str_(record, b"e4").unwrap(), b"\n");
    assert_eq!(str_(record, b"e5").unwrap(), b"q");
    assert_eq!(str_(record, b"e6").unwrap(), b"A");
    assert_eq!(str_(record, b"e7"), Err(CgetError::NotFound));
}

#[test]
fn cgetustr_keeps_the_escapes() {
    let record = b"x:e1=a\\072b:e2=^A:";
    assert_eq!(ustr(record, b"e1").unwrap(), b"a\\072b");
    assert_eq!(ustr(record, b"e2").unwrap(), b"^A");
}

#[test]
fn cgetset_pushes_a_top_record() {
    let (mut db, mut files) = db();
    db.set(Some(b"pushed:am=yes:")).unwrap();
    let record = db.ent(&mut files, &[b"/etc/termcap"], b"pushed").unwrap();
    assert_eq!(record, b"pushed:am=yes:");
    db.set(None).unwrap();
    assert_eq!(
        db.ent(&mut files, &[b"/etc/termcap"], b"pushed"),
        Err(CgetError::NotFound)
    );
}

#[test]
fn cgetnext_walks_every_record() {
    // A file without the loop records: cgetnext would return Loop for those, like cgetent.
    let mut files = FakeFiles::new();
    files.insert(
        b"/etc/caps",
        b"short:tc=long:\nlong:am=yes:\nmissing:tc=nosuch:am=yes:\n",
    );
    let mut db = CapDb::new();
    let mut names = alloc::vec::Vec::new();
    loop {
        match db.next(&mut files, &[b"/etc/caps"]) {
            Ok(record) => {
                let end = record
                    .iter()
                    .position(|&c| c == b':')
                    .unwrap_or(record.len());
                names.push(record[..end].to_vec());
            }
            Err(CgetError::NotFound) => break,
            Err(error) => panic!("{error:?}"),
        }
    }
    assert_eq!(
        names,
        alloc::vec![b"short".to_vec(), b"long".to_vec(), b"missing".to_vec()]
    );
}

#[test]
fn cgetfirst_rewinds() {
    let (mut db, mut files) = db();
    let first = db.first(&mut files, &[b"/etc/termcap"]).unwrap();
    let _ = db.next(&mut files, &[b"/etc/termcap"]).unwrap();
    let again = db.first(&mut files, &[b"/etc/termcap"]).unwrap();
    assert_eq!(first, again);
}

#[test]
fn no_db_file_is_ever_opened() {
    let (mut db, mut files) = db();
    let _ = db.ent(&mut files, &[b"/etc/termcap"], b"short").unwrap();
    assert!(
        files.log.iter().all(|path| !path.ends_with(b".db")),
        "the fake read log contains a .db probe: {:?}",
        files.log
    );
    assert!(files.log.iter().any(|path| path == b"/etc/termcap"));
}

#[test]
fn empty_and_comment_records_are_skipped() {
    let mut files = FakeFiles::new();
    files.insert(b"/etc/caps", b"\n# comment\nreal:am=yes:\n");
    let mut db = CapDb::new();
    assert_eq!(
        db.ent(&mut files, &[b"/etc/caps"], b"real").unwrap(),
        b"real:am=yes:"
    );
}

#[test]
fn shortcap_in_case() {
    // The shape of openbsd:regress/lib/libc/getcap/shortcap.in.
    let mut files = FakeFiles::new();
    files.insert(
        b"shortcap",
        b"sc|shortcap:tc=sclong:\nsclong:am=yes:co#80:\n",
    );
    let mut db = CapDb::new();
    let record = db.ent(&mut files, &[b"shortcap"], b"sc").unwrap();
    assert!(record.windows(4).any(|w| w == b"am=y"));
    assert!(record.windows(4).any(|w| w == b"co#8"));
}
