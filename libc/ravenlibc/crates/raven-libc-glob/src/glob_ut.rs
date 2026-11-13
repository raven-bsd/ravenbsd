// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the glob engine internals.
// Test-only code is exempt from the production lints (17-crate-layout 9.3).
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use super::*;
use crate::locale::Utf8Locale;

const U: Utf8Locale = Utf8Locale;

fn chars(s: &str) -> Vec<Char> {
    let mut out = Vec::new();
    to_chars(&U, s.as_bytes(), &mut out);
    out
}

#[test]
fn char_helpers_strip_the_right_bits() {
    assert_eq!(ch(M_QUOTE | M_PROTECT | 0x41), 0x41);
    assert_eq!(unprot(M_QUOTE | M_PROTECT | 0x41), M_QUOTE | 0x41);
    assert!(ismeta(M_ALL));
    assert!(!ismeta(0x41));
}

#[test]
fn match_literal_and_star() {
    assert!(match_name(&chars("abc"), &chars("abc")));
    assert!(!match_name(&chars("abd"), &chars("abc")));
    assert!(match_name(&chars("abc"), &[M_ALL]));
    assert!(match_name(&[], &[M_ALL]));
    assert!(!match_name(&[], &[M_ONE]));
    assert!(match_name(
        &chars("axbxc"),
        &[ch(Char::from(b'a')), M_ALL, ch(Char::from(b'c'))]
    ));
}

#[test]
fn match_bracket_and_range() {
    let a = ch(Char::from(b'a'));
    let c = ch(Char::from(b'c'));
    let set = [M_SET, a, M_RNG, c, M_END];
    assert!(match_name(&chars("b"), &set));
    assert!(!match_name(&chars("d"), &set));
    let neg = [M_SET, M_NOT, a, M_RNG, c, M_END];
    assert!(match_name(&chars("d"), &neg));
    assert!(!match_name(&chars("b"), &neg));
}

#[test]
fn encode_and_decode_round_trip() {
    let mut out = Vec::new();
    to_chars(&U, "héllo/€".as_bytes(), &mut out);
    assert_eq!(to_bytes(&U, &out), "héllo/€".as_bytes());
}

#[test]
fn invalid_utf8_is_one_literal_byte() {
    let mut out = Vec::new();
    to_chars(&U, b"a\xffb", &mut out);
    assert_eq!(out, vec![ch(Char::from(b'a')), 0xff, ch(Char::from(b'b'))]);
}

#[test]
fn byte_locale_round_trips_every_byte() {
    let loc = crate::locale::ByteLocale;
    let mut out = Vec::new();
    to_chars(&loc, b"\x80\xff", &mut out);
    assert_eq!(to_bytes(&loc, &out), b"\x80\xff");
}
