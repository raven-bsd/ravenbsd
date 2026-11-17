// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//!
//! Host tests for the `mkstemp` template helpers.

use crate::path::mkstemp::{SUFFIX_LEN, encode_suffix, is_template, substitute};

#[test]
fn a_template_ends_with_six_x() {
    assert!(is_template(b"/tmp/tmpXXXXXX"));
    assert!(is_template(b"XXXXXX"));
    assert!(!is_template(b"/tmp/tmpXXXXX"));
    assert!(!is_template(b"/tmp/tmpXXXXXY"));
    assert!(!is_template(b""));
}

#[test]
fn substitution_replaces_only_the_tail() {
    let mut template = *b"/tmp/prefixXXXXXX";
    assert!(substitute(&mut template, b"a1b2c3"));
    assert_eq!(&template, b"/tmp/prefixa1b2c3");
}

#[test]
fn substitution_refuses_a_bad_template() {
    let mut template = *b"/tmp/prefixXXXXXY";
    let original = template;
    assert!(!substitute(&mut template, b"a1b2c3"));
    assert_eq!(template, original);
}

#[test]
fn suffix_uses_the_portable_alphabet() {
    let raw = [0u8, 1, 25, 26, 51, 52];
    let mut out = [0u8; SUFFIX_LEN];
    encode_suffix(&raw, &mut out);
    assert_eq!(&out, b"ABZaz0");
    assert!(out.iter().all(|byte| byte.is_ascii_alphanumeric()));
}

#[test]
fn suffix_masks_the_entropy_into_the_alphabet() {
    let raw = [61u8, 62, 124, 255, 0, 200];
    let mut out = [0u8; SUFFIX_LEN];
    encode_suffix(&raw, &mut out);
    assert_eq!(&out, b"9X8XAI");
}
