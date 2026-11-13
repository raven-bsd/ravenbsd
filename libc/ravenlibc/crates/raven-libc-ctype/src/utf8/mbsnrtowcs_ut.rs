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
fn bounded_input_stops_at_nms() {
    let mut out = [0u32; 8];
    let conv = mbsnrtowcs(None, MbMode::Utf8, &b"abcdef\0"[..3], Some(&mut out[..]));
    assert_eq!(conv.ret, Ok(3));
    assert_eq!(conv.src, SrcPos::At(3));
    assert_eq!(&out[..3], &[0x61, 0x62, 0x63]);
}

#[test]
fn truncated_character_is_consumed_without_error() {
    let mut out = [0u32; 8];
    let conv = mbsnrtowcs(
        None,
        MbMode::Utf8,
        &[0xe2, 0x82, 0xac][..2],
        Some(&mut out[..]),
    );
    assert_eq!(conv.ret, Ok(0));
    assert_eq!(conv.src, SrcPos::At(2));
    assert!(!conv.state.is_initial());
}

#[test]
fn nul_terminates_and_is_not_counted() {
    let mut out = [0u32; 8];
    let conv = mbsnrtowcs(None, MbMode::Utf8, b"a\0b", Some(&mut out[..]));
    assert_eq!(conv.ret, Ok(1));
    assert_eq!(conv.src, SrcPos::Nul);
    assert_eq!(out[0], 0x61);
    assert_eq!(out[1], 0);
}

#[test]
fn count_only_reports_no_advance() {
    let conv = mbsnrtowcs(None, MbMode::Utf8, &b"abc\0"[..4], None);
    assert_eq!(conv.ret, Ok(3));
    assert_eq!(conv.src, SrcPos::At(0));
}
