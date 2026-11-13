// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! `localeconv` tests.

use super::*;

#[test]
fn the_static_c_locale_lconv() {
    let value = localeconv();
    assert_eq!(value.decimal_point, b".\0");
    assert_eq!(value.thousands_sep, b"\0");
    assert_eq!(value.grouping, b"\0");
    assert_eq!(value.int_frac_digits, i8::MAX);
    assert_eq!(value.frac_digits, i8::MAX);
    assert_eq!(value.p_cs_precedes, i8::MAX);
    assert_eq!(value.n_sign_posn, i8::MAX);
    assert_eq!(value.int_n_sign_posn, i8::MAX);
}
