// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of the magic guards.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use crate::consts::REG_EXTENDED;
use crate::locale::ByteLocale;

#[test]
fn a_compiled_regex_is_valid() {
    let re = crate::regcomp(b"a", REG_EXTENDED, &ByteLocale)
        .unwrap_or_else(|error| panic!("regcomp {}", error.0));
    assert!(re.magic_ok());
}

#[test]
fn a_zeroed_regex_is_invalid() {
    let re = crate::Regex::zeroed_for_test();
    assert!(!re.magic_ok());
}

#[test]
fn dropping_clears_the_magic_of_the_value() {
    let mut re = crate::regcomp(b"a", REG_EXTENDED, &ByteLocale)
        .unwrap_or_else(|error| panic!("regcomp {}", error.0));
    assert!(re.magic_ok());
    re.magic1 = 0;
    assert!(!re.magic_ok());
}
