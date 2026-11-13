// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use crate::ctx::DtoaCtx;
use crate::{C_NUMERIC, DigitMode, Rounding};

fn digits(v: f64, mode: DigitMode, ndigits: i32) -> (String, i32) {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 800];
    let d = crate::dtoa(&mut ctx, v, mode, ndigits, Rounding::Nearest, &mut out).unwrap();
    (String::from_utf8(out[..d.len].to_vec()).unwrap(), d.decpt)
}

#[test]
fn known_shortest_values() {
    assert_eq!(digits(1.5, DigitMode::Shortest, 0), ("15".to_string(), 1));
    assert_eq!(digits(0.1, DigitMode::Shortest, 0), ("1".to_string(), 0));
    assert_eq!(digits(1e23, DigitMode::Shortest, 0), ("1".to_string(), 24));
    assert_eq!(
        digits(123456789.0, DigitMode::Shortest, 0),
        ("123456789".to_string(), 9)
    );
    assert_eq!(
        digits(f64::from_bits(1), DigitMode::Shortest, 0),
        ("5".to_string(), -323)
    );
    let (max_digits, max_decpt) = digits(f64::MAX, DigitMode::Shortest, 0);
    assert_eq!(max_digits, "17976931348623157");
    assert_eq!(max_decpt, 309);
    let _ = C_NUMERIC;
}

#[test]
fn sign_and_special_values() {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 800];
    let z = crate::dtoa(
        &mut ctx,
        -0.0,
        DigitMode::Shortest,
        0,
        Rounding::Nearest,
        &mut out,
    )
    .unwrap();
    assert_eq!(z.kind, crate::Kind::Zero);
    assert!(z.negative);
    assert_eq!(&out[..z.len], b"0");
    assert_eq!(z.decpt, 1);
    let inf = crate::dtoa(
        &mut ctx,
        f64::INFINITY,
        DigitMode::Shortest,
        0,
        Rounding::Nearest,
        &mut out,
    )
    .unwrap();
    assert_eq!(inf.kind, crate::Kind::Inf);
    assert_eq!(inf.len, 0);
    let nan = crate::dtoa(
        &mut ctx,
        -f64::NAN,
        DigitMode::Shortest,
        0,
        Rounding::Nearest,
        &mut out,
    )
    .unwrap();
    assert_eq!(nan.kind, crate::Kind::Nan);
    assert!(nan.negative);
    assert_eq!(nan.len, 0);
}

#[test]
fn buffer_too_small_reports_needed() {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 2];
    let err = crate::dtoa(
        &mut ctx,
        1.5,
        DigitMode::SigDigits,
        10,
        Rounding::Nearest,
        &mut out,
    )
    .unwrap_err();
    assert_eq!(err, crate::DtoaError::BufferTooSmall { needed: 10 });
}
