// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::*;
use crate::ctx::DtoaCtx;
use crate::{C_NUMERIC, Rounding};

fn parse(text: &[u8]) -> (f64, usize, i32) {
    let mut ctx = DtoaCtx::new();
    strtod(&mut ctx, text, &C_NUMERIC, Rounding::Nearest).unwrap()
}

#[test]
fn basic_values() {
    let (v, n, flags) = parse(b"1.23");
    assert_eq!(n, 4);
    assert_eq!(v.to_bits(), 0x3ff3_ae14_7ae1_47ae);
    assert_eq!(flags, 0);
    assert_eq!(parse(b"0.1").0.to_bits(), 0x3fb9_9999_9999_999a);
    assert_eq!(parse(b"1e23").0, 1e23);
    assert_eq!(parse(b"-0.0").0.to_bits(), 0x8000_0000_0000_0000);
}

#[test]
fn no_conversion_consumes_nothing() {
    for text in [b"".as_slice(), b"   ".as_slice(), b"e1".as_slice()] {
        let (v, n, flags) = parse(text);
        assert_eq!(n, 0);
        assert_eq!(v.to_bits(), 0);
        assert_eq!(flags, 0);
    }
}

#[test]
fn partial_consumption() {
    assert_eq!(parse(b"0x").1, 1);
    assert_eq!(parse(b"1e").1, 1);
    assert_eq!(parse(b"1e+").1, 1);
    assert_eq!(parse(b"nan(abc)").1, 8);
    assert_eq!(parse(b"nan(abc").1, 3);
    assert_eq!(parse(b"1,5").1, 1);
}

#[test]
fn overflow_and_underflow_flags() {
    let (v, _, flags) = parse(b"1e999");
    assert!(v.is_infinite());
    assert!(flags & STRTOG_OVERFLOW != 0);
    let (v, _, flags) = parse(b"1e-999");
    assert_eq!(v, 0.0);
    assert!(flags & STRTOG_UNDERFLOW != 0);
    let (v, _, flags) = parse(b"5e-324");
    assert_eq!(v.to_bits(), 1);
    // The donor's ERANGE test is "exponent field == 0", so a denormal result carries UNDERFLOW
    // even when it is exact (`strtod.c:1081-1090`, contract rule 3).
    assert!(flags & STRTOG_UNDERFLOW != 0);
}

#[test]
fn leading_fractional_zeros_are_not_digits() {
    // Regression: the dedicated `strtod.c` path skipped the leading zeros but forgot to fold the
    // count into `nf`, so they entered y/z as digits (19 digits instead of 16 significant ones).
    let (v, n, flags) = parse(b"0.0008057503634850169");
    assert_eq!(n, 21);
    assert_eq!(v.to_bits(), 0x3f4a_671f_bade_461c);
    assert!(!flags_underflow_or_overflow(flags));
}

fn flags_underflow_or_overflow(flags: i32) -> bool {
    flags & (crate::types::STRTOG_OVERFLOW | crate::types::STRTOG_UNDERFLOW) != 0
}

#[test]
fn signed_zero_keeps_its_sign() {
    for text in [b"-0".as_slice(), b"-0.0".as_slice(), b"-0000".as_slice()] {
        let (v, n, flags) = parse(text);
        assert_eq!(n, text.len(), "consumed for {text:?}");
        assert_eq!(v.to_bits(), (-0.0f64).to_bits(), "sign for {text:?}");
        assert_eq!(flags, 0);
    }
    assert_eq!(parse(b"+0").0.to_bits(), 0);
}
