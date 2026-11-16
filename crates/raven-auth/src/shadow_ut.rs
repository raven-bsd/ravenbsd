// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

const ROOT_HASH: &str = "$6$r4ven$S6VNu6h5bJJ4N0puQ63budZes8.I7oATcnLzSQBmkTcTvZ.4QTdkbKttZybkfYqCmRZvvzbfDzxeMs3FalGIQ.";

#[test]
fn a_root_line_round_trips() {
    let line = root_line(ROOT_HASH);
    let entry = ShadowEntry::parse(&line).expect("parse");
    assert_eq!(entry.name, "root");
    assert_eq!(entry.hash, ROOT_HASH);
    assert_eq!(entry.last_change, 0);
    assert_eq!(entry.max, 99_999);
    assert_eq!(entry.warn, 7);
    assert_eq!(entry.render(), line);
}

#[test]
fn empty_numeric_fields_are_zero() {
    let entry = ShadowEntry::parse("root:x:::::::").expect("parse");
    assert_eq!(entry.last_change, 0);
    assert_eq!(entry.min, 0);
    assert_eq!(entry.max, 0);
    assert_eq!(entry.warn, 0);
    assert_eq!(entry.inactive, 0);
    assert_eq!(entry.expire, 0);
    assert_eq!(entry.reserved, 0);
}

#[test]
fn bad_field_counts_are_rejected() {
    assert_eq!(
        ShadowEntry::parse("root:x:0"),
        Err(AuthError::InvalidRecord)
    );
    assert_eq!(
        ShadowEntry::parse("root:x:0:0:0:0:0:0:0:extra"),
        Err(AuthError::InvalidRecord)
    );
    assert_eq!(
        ShadowEntry::parse(":x:0:0:0:0:0:0:0"),
        Err(AuthError::InvalidRecord)
    );
}

#[test]
fn bad_numbers_are_rejected() {
    assert_eq!(
        ShadowEntry::parse("root:x:not-a-day:0:0:0:0:0:0"),
        Err(AuthError::InvalidNumber)
    );
}

#[test]
fn the_default_shadow_verifies_root_only() {
    let line = default_root_shadow();
    assert!(verify_root_shadow(&line, "raven"));
    assert!(!verify_root_shadow(&line, "wrong"));
    let other = line.replacen("root:", "nobody:", 1);
    assert!(!verify_root_shadow(&other, "raven"));
}

#[test]
fn a_known_vector_verifies() {
    // Generated with `openssl passwd -6 -salt r4ven raven`; the low-level vector is
    // independent of this crate's high-level salt encoding.
    assert!(verify_password("raven", ROOT_HASH));
    assert!(!verify_password("raven!", ROOT_HASH));
}

#[test]
fn locked_hashes_never_verify() {
    let entry = ShadowEntry::parse("root:!$6$whatever:0:0:0:0:0:0:0").expect("parse");
    assert!(!entry.verify("root", "whatever"));
    let entry = ShadowEntry::parse("root:*:0:0:0:0:0:0:0").expect("parse");
    assert!(!entry.verify("root", "whatever"));
}
