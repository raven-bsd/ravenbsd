// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic
)]

use super::*;

#[test]
fn bounded_wide_count_limits_conversion() {
    let src = [0x61u32, 0x62, 0x63, 0];
    let mut out = [0u8; 8];
    let conv = wcsnrtombs(None, MbMode::Utf8, &src[..2], Some(&mut out[..]));
    assert_eq!(conv.ret, Ok(2));
    assert_eq!(conv.src, SrcPos::At(2));
    assert_eq!(&out[..2], &[0x61, 0x62]);
}

#[test]
fn nul_consumes_one_byte_but_counts_zero() {
    let src = [0x61u32, 0];
    let mut out = [0u8; 8];
    let conv = wcsnrtombs(None, MbMode::Utf8, &src[..2], Some(&mut out[..]));
    assert_eq!(conv.ret, Ok(1));
    assert_eq!(conv.src, SrcPos::Nul);
    assert_eq!(&out[..2], &[0x61, 0]);
}

#[test]
fn short_byte_limit_stops_before_a_partial_character() {
    let src = [0x20acu32, 0];
    let mut out = [0u8; 4];
    let conv = wcsnrtombs(None, MbMode::Utf8, &src[..1], Some(&mut out[..2]));
    assert_eq!(conv.ret, Ok(0));
    assert_eq!(conv.src, SrcPos::At(0));
}

#[test]
fn count_only_returns_byte_length() {
    let src = [0x20acu32, 0];
    let conv = wcsnrtombs(None, MbMode::Utf8, &src[..1], None);
    assert_eq!(conv.ret, Ok(3));
}
