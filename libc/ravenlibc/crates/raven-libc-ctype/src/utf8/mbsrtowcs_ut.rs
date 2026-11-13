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
fn count_only_does_not_report_an_advance() {
    let conv = mbsrtowcs(None, MbMode::Utf8, b"abc\0", None);
    assert_eq!(conv.ret, Ok(3));
    assert_eq!(conv.src, SrcPos::At(0));
}

#[test]
fn writes_until_the_destination_is_full() {
    let mut out = [0u32; 2];
    let conv = mbsrtowcs(None, MbMode::Utf8, b"abcd\0", Some(&mut out[..]));
    assert_eq!(conv.ret, Ok(2));
    assert_eq!(conv.src, SrcPos::At(2));
    assert_eq!(out, [0x61, 0x62]);

    let mut out2 = [0u32; 8];
    let conv = mbsrtowcs(None, MbMode::Utf8, b"abcd\0", Some(&mut out2[..]));
    assert_eq!(conv.ret, Ok(4));
    assert_eq!(conv.src, SrcPos::Nul);
    assert_eq!(&out2[..4], &[0x61, 0x62, 0x63, 0x64]);
}

#[test]
fn error_position_is_the_lead_byte() {
    let mut out = [0u32; 8];
    let conv = mbsrtowcs(None, MbMode::Utf8, b"ab\xffcd\0", Some(&mut out[..]));
    assert_eq!(conv.ret, Err(MbError::Ilseq));
    assert_eq!(conv.src, SrcPos::At(2));
    assert_eq!(&out[..2], &[0x61, 0x62]);
    assert!(conv.state.is_initial());
}

#[test]
fn byte_mode_passes_high_bytes_through() {
    let mut out = [0u32; 4];
    let conv = mbsrtowcs(None, MbMode::Byte, &[0x41, 0xe9, 0], Some(&mut out[..]));
    assert_eq!(conv.ret, Ok(2));
    assert_eq!(conv.src, SrcPos::Nul);
    assert_eq!(&out[..2], &[0x41, 0xdfe9]);
}
