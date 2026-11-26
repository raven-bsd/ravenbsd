// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for `/etc/shadow` records.

use super::*;

const ROOT: &str = "root:$6$rounds=5000$G3aRZtaEHFI9mxqPo3H6$NWDGRfLCpNbICW5P.FtjRWkSeV2R0JxL7QOLj/As2KydiD3R3HfBVGeHPfXTfAgrbrIlubqza9MTtpRUfXjv8.:0:0:99999:7:0:0:0";

#[test]
fn a_record_round_trips() {
    let entry = ShadowEntry::parse(ROOT, 1).expect("root");
    assert_eq!(entry.name, "root");
    assert_eq!(entry.last_change, 0);
    assert_eq!(entry.max, 99999);
    assert_eq!(entry.render(), ROOT);
}

#[test]
fn empty_numbers_follow_shadow_defaults() {
    let entry = ShadowEntry::parse("bob:!:19000::7::::", 1).expect("bob");
    assert_eq!(entry.min, 0);
    assert_eq!(entry.inactive, -1);
    assert_eq!(entry.expire, -1);
    assert_eq!(entry.reserved, 0);
}

#[test]
fn a_new_locked_record_renders_minus_one() {
    let entry = ShadowEntry::locked("bob");
    assert_eq!(entry.inactive, -1);
    assert_eq!(entry.expire, -1);
    assert!(entry.render().ends_with(":0:0:99999:7:-1:-1:0"));
    let parsed = ShadowEntry::parse(&entry.render(), 1).expect("round trip");
    assert_eq!(parsed.inactive, -1);
    assert_eq!(parsed.expire, -1);
    assert_eq!(parsed.render(), entry.render());
}

#[test]
fn negative_or_non_numeric_fields_are_rejected() {
    for line in [
        "bob:!:19000:-1:7:0:0:0:0",
        "bob:!:19000:x:7:0:0:0:0",
        "bob:!:19000:0:7:0:-2:0:0",
        "bob:plain:1:2:3:4:5:6:7",
    ] {
        assert!(ShadowEntry::parse(line, 1).is_err(), "{line}");
    }
}

#[test]
fn a_record_has_exactly_nine_fields() {
    let error = ShadowEntry::parse("bob:!:19000:0:99999:7:0:0", 4).expect_err("short");
    assert_eq!(
        error,
        DbError::FieldCount {
            file: DbFile::Shadow,
            line: 4,
            expected: 9,
            found: 8
        }
    );
}

#[test]
fn lock_and_unlock_round_trip() {
    let mut entry = ShadowEntry::parse(ROOT, 1).expect("root");
    entry.lock();
    assert!(entry.is_locked());
    assert!(entry.hash.starts_with('!'));
    entry.unlock().expect("unlock");
    assert_eq!(entry.hash, raven_auth::DEFAULT_ROOT_HASH);
}

#[test]
fn a_star_record_cannot_be_unlocked() {
    let mut entry = ShadowEntry::locked("bob");
    entry.hash = "*".to_string();
    assert!(entry.unlock().is_err());
}

#[test]
fn the_parser_agrees_with_raven_auth() {
    let ours = ShadowEntry::parse(ROOT, 1).expect("ours");
    let theirs = raven_auth::ShadowEntry::parse(ROOT).expect("theirs");
    assert_eq!(ours.name, theirs.name);
    assert_eq!(ours.hash, theirs.hash);
    assert_eq!(ours.last_change, theirs.last_change);
    assert_eq!(ours.render(), theirs.render());
}
