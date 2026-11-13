// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the crate surface.
// Test-only code is exempt from the production lints (17-crate-layout 9.3).
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use super::*;

#[test]
fn public_constants_are_reachable() {
    assert_eq!(consts::FNM_CASEFOLD, 0x10);
    assert_eq!(consts::GLOB_LIMIT, 0x1000);
    assert_eq!(consts::WRDE_SYNTAX, 6);
}

#[test]
fn locales_are_usable_through_the_trait_object() {
    let utf8: &dyn PatternLocale = &Utf8Locale;
    assert_eq!(utf8.mb_cur_max(), 4);
    let byte: &dyn PatternLocale = &ByteLocale;
    assert_eq!(byte.mb_cur_max(), 1);
    assert_eq!(byte.mbtowc(b"\xff"), MbResult::Char { wc: 0xdfff, len: 1 });
}

#[test]
fn globstate_defaults_are_empty() {
    let state = GlobState::default();
    assert!(state.pathv.is_empty());
    assert_eq!(state.offs, 0);
    assert_eq!(state.matchc, 0);
    assert_eq!(state.flags, 0);
    assert_eq!(state.errno, 0);
}
