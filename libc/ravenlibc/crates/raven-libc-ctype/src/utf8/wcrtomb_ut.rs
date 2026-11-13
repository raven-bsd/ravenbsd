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
fn utf8_lengths_and_boundaries() {
    let mut out = [0u8; 4];
    assert_eq!(wcrtomb(MbMode::Utf8, 0x7f, &mut out), Ok(1));
    assert_eq!(out[0], 0x7f);
    assert_eq!(wcrtomb(MbMode::Utf8, 0x80, &mut out), Ok(2));
    assert_eq!(&out[..2], &[0xc2, 0x80]);
    assert_eq!(wcrtomb(MbMode::Utf8, 0x7ff, &mut out), Ok(2));
    assert_eq!(wcrtomb(MbMode::Utf8, 0x800, &mut out), Ok(3));
    assert_eq!(wcrtomb(MbMode::Utf8, 0xffff, &mut out), Ok(3));
    assert_eq!(wcrtomb(MbMode::Utf8, 0x10000, &mut out), Ok(4));
    assert_eq!(wcrtomb(MbMode::Utf8, 0x10ffff, &mut out), Ok(4));
}

#[test]
fn surrogates_and_out_of_range_are_ilseq() {
    let mut out = [0u8; 4];
    assert_eq!(wcrtomb(MbMode::Utf8, 0xd800, &mut out), Err(MbError::Ilseq));
    assert_eq!(wcrtomb(MbMode::Utf8, 0xdfff, &mut out), Err(MbError::Ilseq));
    assert_eq!(
        wcrtomb(MbMode::Utf8, 0x110000, &mut out),
        Err(MbError::Ilseq)
    );
    assert_eq!(
        wcrtomb(MbMode::Utf8, u32::MAX, &mut out),
        Err(MbError::Ilseq)
    );
}

#[test]
fn byte_mode_encodes_code_units_only() {
    let mut out = [0u8; 4];
    assert_eq!(wcrtomb(MbMode::Byte, 0x41, &mut out), Ok(1));
    assert_eq!(out[0], 0x41);
    assert_eq!(wcrtomb(MbMode::Byte, 0xdf80, &mut out), Ok(1));
    assert_eq!(out[0], 0x80);
    assert_eq!(wcrtomb(MbMode::Byte, 0xdfff, &mut out), Ok(1));
    assert_eq!(out[0], 0xff);
    assert_eq!(wcrtomb(MbMode::Byte, 0x100, &mut out), Err(MbError::Ilseq));
}
