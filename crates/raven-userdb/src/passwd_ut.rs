// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `/etc/passwd` records.

use super::*;

const ALICE: &str = "alice:x:1000:1000:Alice Example:/home/alice:/bin/brush";

#[test]
fn a_record_round_trips() {
    let entry = PasswdEntry::parse(ALICE, 2).expect("alice");
    assert_eq!(entry.name, "alice");
    assert_eq!(entry.uid, 1000);
    assert_eq!(entry.gid, 1000);
    assert_eq!(entry.gecos, "Alice Example");
    assert_eq!(entry.home, "/home/alice");
    assert_eq!(entry.shell, "/bin/brush");
    assert_eq!(entry.render(), ALICE);
}

#[test]
fn nobody_has_empty_like_paths() {
    let line = "nobody:x:65534:65534:nobody:/nonexistent:/nonexistent";
    let entry = PasswdEntry::parse(line, 4).expect("nobody");
    assert_eq!(entry.render(), line);
}

#[test]
fn field_counts_are_exact() {
    let error = PasswdEntry::parse("alice:x:1000", 9).expect_err("short");
    assert_eq!(
        error,
        DbError::FieldCount {
            file: DbFile::Passwd,
            line: 9,
            expected: 7,
            found: 3
        }
    );
}

#[test]
fn the_password_field_must_be_x() {
    let line = "alice:secret:1000:1000::/home/alice:/bin/brush";
    assert!(matches!(
        PasswdEntry::parse(line, 1),
        Err(DbError::InvalidField {
            field: "passwd",
            ..
        })
    ));
}

#[test]
fn ids_are_decimal() {
    let line = "alice:x:ten:1000::/home/alice:/bin/brush";
    assert!(PasswdEntry::parse(line, 1).is_err());
}

#[test]
fn paths_are_absolute() {
    let line = "alice:x:1000:1000::home/alice:/bin/brush";
    assert!(PasswdEntry::parse(line, 1).is_err());
}
