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

#[test]
fn binary128_one_point_five_layout() {
    let mut ctx = DtoaCtx::new();
    let (v, n, _) = strtor_q(&mut ctx, b"1.5", &C_NUMERIC, Rounding::Nearest).unwrap();
    assert_eq!(n, 3);
    assert_eq!(v.0[0..13], [0u8; 13]);
    assert_eq!(v.0[13], 0x80);
    assert_eq!(v.0[14], 0xff);
    assert_eq!(v.0[15], 0x3f);
}

#[test]
fn ultoq_special_patterns() {
    let inf = ultoq(&[0, 0, 0, 0], 0, STRTOG_INFINITE);
    assert_eq!(&inf.0[12..], &[0x00, 0x00, 0xff, 0x7f]);
    let nan = ultoq(&[0, 0, 0, 0], 0, STRTOG_NAN);
    assert_eq!(&nan.0[12..], &[0x00, 0x80, 0xff, 0x7f]);
    let normal = ultoq(&[0, 0, 0, 0], -112, STRTOG_NORMAL);
    assert_eq!(normal.0[15], 0x3f);
    assert_eq!(normal.0[14], 0xff);
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn text_of(digits: &[u8], decpt: i32, negative: bool) -> Vec<u8> {
    let mut text = Vec::with_capacity(digits.len() + 8);
    if negative {
        text.push(b'-');
    }
    text.push(digits[0]);
    if digits.len() > 1 {
        text.push(b'.');
        text.extend_from_slice(&digits[1..]);
    }
    text.push(b'e');
    text.extend_from_slice(format!("{}", decpt - 1).as_bytes());
    text
}

fn round_trip_ld128(bytes: [u8; 16]) -> bool {
    let value = LongDouble(bytes);
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 64];
    let d = match crate::ldtoa::ldtoa_ld128(
        &mut ctx,
        value,
        crate::DigitMode::Shortest.as_mode(),
        0,
        Rounding::Nearest,
        &mut out,
    ) {
        Ok(d) => d,
        Err(crate::DtoaError::NoMemory) => {
            // The smallest binary128 subnormals need powers of five beyond the 8 KiB arena;
            // without the alloc spill this is the documented NoMemory path.
            #[cfg(feature = "alloc")]
            panic!("spill should have handled {bytes:02x?}");
            #[cfg(not(feature = "alloc"))]
            return false;
        }
        Err(error) => panic!("shortest ld128 {error:?} for {bytes:02x?}"),
    };
    if d.kind == crate::Kind::Nan {
        assert_eq!(d.len, 0);
        return true;
    }
    if d.kind == crate::Kind::Inf {
        assert_eq!(d.len, 0);
        return true;
    }
    let text = text_of(&out[..d.len], d.decpt, d.negative);
    let mut ctx = DtoaCtx::new();
    let (back, consumed, _) = strtor_q(&mut ctx, &text, &C_NUMERIC, Rounding::Nearest).unwrap();
    assert_eq!(
        consumed,
        text.len(),
        "shortest ld128 consumed for {bytes:02x?}"
    );
    assert_eq!(back.0, bytes, "shortest ld128 round trip for {bytes:02x?}");

    let mut ctx = DtoaCtx::new();
    let d36 = match crate::ldtoa::ldtoa_ld128(
        &mut ctx,
        value,
        crate::DigitMode::SigDigits.as_mode(),
        36,
        Rounding::Nearest,
        &mut out,
    ) {
        Ok(d) => d,
        Err(crate::DtoaError::NoMemory) => {
            #[cfg(feature = "alloc")]
            panic!("spill should have handled {bytes:02x?}");
            #[cfg(not(feature = "alloc"))]
            return false;
        }
        Err(error) => panic!("36-digit ld128 {error:?} for {bytes:02x?}"),
    };
    let text = text_of(&out[..d36.len], d36.decpt, d36.negative);
    let mut ctx = DtoaCtx::new();
    let (back, consumed, _) = strtor_q(&mut ctx, &text, &C_NUMERIC, Rounding::Nearest).unwrap();
    assert_eq!(
        consumed,
        text.len(),
        "36-digit ld128 consumed for {bytes:02x?}"
    );
    assert_eq!(back.0, bytes, "36-digit ld128 round trip for {bytes:02x?}");
    true
}

#[test]
fn binary128_property_moderate_range() {
    let mut state = 0x1234_5678_9abc_def0u64;
    let mut checked = 0usize;
    for _ in 0..100_000 {
        let lo = splitmix64(&mut state);
        let hi = splitmix64(&mut state);
        let mut raw = (u128::from(hi) << 64) | u128::from(lo);
        raw &= !(0x7fffu128 << 112);
        raw |= 0x3f80u128 << 112; // exponents around 1.0, no spill
        let bytes = raw.to_le_bytes();
        if round_trip_ld128(bytes) {
            checked += 1;
        }
    }
    assert_eq!(checked, 100_000);
}

#[cfg(feature = "alloc")]
#[test]
fn binary128_boundaries_round_trip() {
    let cases: [[u8; 16]; 8] = [
        [0; 16],                                             // +0
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x80], // -0
        [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],    // min subnormal
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 0],    // min normal
        [
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xef, 0x7f,
        ], // one ulp below 2^16369 (drop_down regression)
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xf0, 0x7f], // +inf
        [1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xf0, 0x7f], // qNaN
        [
            0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff,
        ], // -NaN
    ];
    for bytes in cases {
        let _ = round_trip_ld128(bytes);
    }
}
