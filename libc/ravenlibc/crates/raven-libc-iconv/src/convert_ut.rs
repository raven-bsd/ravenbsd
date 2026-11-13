// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! Converter tests.

use super::*;

#[test]
fn utf8_to_utf32_round_trip() {
    let mut converter = Iconv::open(b"UTF-32LE", b"UTF-8").expect("open");
    let mut input: &[u8] = "A\u{20ac}\u{1f600}".as_bytes();
    let mut buffer = [0u8; 32];
    let mut output: &mut [u8] = &mut buffer;
    assert_eq!(converter.convert(&mut input, &mut output), Ok(0));
    assert!(input.is_empty());
    let written = 32 - output.len();
    assert_eq!(written, 12);
    let mut back = Iconv::open(b"UTF-8", b"UTF-32LE").expect("back");
    let mut encoded = &buffer[..written];
    let mut decoded = [0u8; 32];
    let mut decoded_out: &mut [u8] = &mut decoded;
    assert_eq!(back.convert(&mut encoded, &mut decoded_out), Ok(0));
    let size = 32 - decoded_out.len();
    assert_eq!(&decoded[..size], "A\u{20ac}\u{1f600}".as_bytes());
}

#[test]
fn ascii_translit_and_strict() {
    let mut strict = Iconv::open(b"ASCII", b"UTF-8").expect("open");
    let mut input: &[u8] = "A\u{20ac}".as_bytes();
    let mut buffer = [0u8; 8];
    let mut output: &mut [u8] = &mut buffer;
    assert_eq!(
        strict.convert(&mut input, &mut output),
        Err(IconvError::IllegalSequence)
    );
    assert_eq!(input, "\u{20ac}".as_bytes());

    let mut translit = Iconv::open(b"ASCII//TRANSLIT", b"UTF-8").expect("open");
    let mut input: &[u8] = "A\u{20ac}".as_bytes();
    let mut buffer = [0u8; 8];
    let mut output: &mut [u8] = &mut buffer;
    assert_eq!(translit.convert(&mut input, &mut output), Ok(1));
    assert_eq!(&buffer[..2], b"A?");
}

#[test]
fn latin1_maps_the_upper_half() {
    let mut converter = Iconv::open(b"UTF-8", b"ISO-8859-1").expect("open");
    let mut input: &[u8] = &[0xe9];
    let mut buffer = [0u8; 8];
    let mut output: &mut [u8] = &mut buffer;
    assert_eq!(converter.convert(&mut input, &mut output), Ok(0));
    assert_eq!(&buffer[..2], "\u{e9}".as_bytes());
}

#[test]
fn utf16_target_has_no_bom() {
    let mut converter = Iconv::open(b"UTF-16", b"UTF-8").expect("open");
    let mut input: &[u8] = b"A";
    let mut buffer = [0u8; 4];
    let mut output: &mut [u8] = &mut buffer;
    assert_eq!(converter.convert(&mut input, &mut output), Ok(0));
    assert_eq!(&buffer[..2], &[0x00, 0x41]);
}

#[test]
fn incomplete_and_illegal_positions() {
    let mut converter = Iconv::open(b"UTF-8", b"UTF-8").expect("open");
    let mut input: &[u8] = &[0xe2, 0x82];
    let mut buffer = [0u8; 4];
    let mut output: &mut [u8] = &mut buffer;
    assert_eq!(
        converter.convert(&mut input, &mut output),
        Err(IconvError::Incomplete)
    );
    assert_eq!(input, &[0xe2, 0x82]);
    let mut input: &[u8] = &[0xff];
    assert_eq!(
        converter.convert(&mut input, &mut output),
        Err(IconvError::IllegalSequence)
    );
    assert_eq!(input, &[0xff]);
}
