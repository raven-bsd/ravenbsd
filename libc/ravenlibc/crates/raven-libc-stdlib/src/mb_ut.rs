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

//! The `mb` adapter table: every C39 result shape mapped to the C return value.

use raven_libc_ctype::MbMode;

use super::*;

#[test]
fn null_argument_returns_zero() {
    assert_eq!(mblen(None, MbMode::Utf8), 0);
    assert_eq!(mbtowc(None, MbMode::Utf8), (0, 0));
}

#[test]
fn empty_input_returns_minus_one() {
    assert_eq!(mblen(Some(b""), MbMode::Utf8), -1);
    assert_eq!(mbtowc(Some(b""), MbMode::Utf8), (-1, 0));
}

#[test]
fn embedded_nul_returns_zero() {
    assert_eq!(mblen(Some(b"\0abc"), MbMode::Utf8), 0);
    assert_eq!(mbtowc(Some(b"\0abc"), MbMode::Utf8), (0, 0));
}

#[test]
fn utf8_and_byte_modes() {
    let text = "é".as_bytes();
    assert_eq!(mblen(Some(text), MbMode::Utf8), 2);
    assert_eq!(mbtowc(Some(text), MbMode::Utf8), (2, 0xe9));
    assert_eq!(mblen(Some(text), MbMode::Byte), 1);
    // C39 uses musl's C-locale code-unit range 0xDF80..0xDFFF for single bytes.
    assert_eq!(mbtowc(Some(text), MbMode::Byte), (1, 0xdfc3));
}

#[test]
fn invalid_and_truncated_inputs() {
    assert_eq!(mblen(Some(&[0x80]), MbMode::Utf8), -1);
    assert_eq!(mbtowc(Some(&[0xc3]), MbMode::Utf8), (-1, 0));
    assert_eq!(mblen(Some(&[0xed, 0xa0, 0x80]), MbMode::Utf8), -1);
}

#[test]
fn wctomb_both_modes() {
    let mut out = [0u8; 4];
    assert_eq!(wctomb(0x41, MbMode::Utf8, &mut out), 1);
    assert_eq!(out[0], b'A');
    assert_eq!(wctomb(0xe9, MbMode::Utf8, &mut out), 2);
    assert_eq!(&out[..2], &[0xc3, 0xa9]);
    assert_eq!(wctomb(0xdfe9, MbMode::Byte, &mut out), 1);
    assert_eq!(out[0], 0xe9);
    assert_eq!(wctomb(0xe9, MbMode::Byte, &mut out), -1);
    assert_eq!(wctomb(0xd800, MbMode::Utf8, &mut out), -1);
}

#[test]
fn ascii_roundtrip_both_modes() {
    let mut out = [0u8; 4];
    for byte in *b"A0 ~" {
        assert_eq!(wctomb(u32::from(byte), MbMode::Utf8, &mut out), 1);
        assert_eq!(out[0], byte);
        assert_eq!(mbtowc(Some(&out[..1]), MbMode::Utf8), (1, u32::from(byte)));
    }
}

#[test]
fn four_byte_utf8() {
    let text = "😀".as_bytes();
    assert_eq!(mblen(Some(text), MbMode::Utf8), 4);
    assert_eq!(mbtowc(Some(text), MbMode::Utf8).1, 0x1f600);
}

#[test]
fn overlong_and_surrogate_rejected() {
    assert_eq!(mblen(Some(&[0xc0, 0x80]), MbMode::Utf8), -1);
    assert_eq!(mblen(Some(&[0xed, 0xb0, 0x80]), MbMode::Utf8), -1);
}

#[test]
fn c_locale_bytes_are_codeunits() {
    let mut out = [0u8; 4];
    assert_eq!(wctomb(0xdfff, MbMode::Byte, &mut out), 1);
    assert_eq!(out[0], 0xff);
    assert_eq!(mbtowc(Some(&[0xff]), MbMode::Byte), (1, 0xdfff));
}
