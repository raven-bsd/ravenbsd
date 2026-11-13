// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of the collating-element table.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

#[test]
fn table_has_ninety_five_entries() {
    assert_eq!(CNAMES.len(), 95);
}

#[test]
fn known_names_map_to_their_bytes() {
    assert_eq!(lookup(b"NUL"), Some(0));
    assert_eq!(lookup(b"newline"), Some(10));
    assert_eq!(lookup(b"tab"), Some(9));
    assert_eq!(lookup(b"space"), Some(32));
    assert_eq!(lookup(b"DEL"), Some(127));
    assert_eq!(lookup(b"backslash"), Some(b'\\'));
    assert_eq!(lookup(b"reverse-solidus"), Some(b'\\'));
}

#[test]
fn lookup_requires_the_exact_name() {
    assert_eq!(lookup(b"N"), None);
    assert_eq!(lookup(b"NU"), None);
    assert_eq!(lookup(b"NULL"), None);
    assert_eq!(lookup(b"nul"), None);
    assert_eq!(lookup(b""), None);
}

#[test]
fn duplicate_spellings_are_kept() {
    assert_eq!(lookup(b"BEL"), lookup(b"alert"));
    assert_eq!(lookup(b"hyphen"), lookup(b"hyphen-minus"));
    assert_eq!(lookup(b"period"), lookup(b"full-stop"));
    assert_eq!(lookup(b"left-brace"), lookup(b"left-curly-bracket"));
}

#[test]
fn codes_are_bytes_in_order() {
    assert!(CNAMES.windows(2).all(|pair| pair[0].code <= pair[1].code));
}
