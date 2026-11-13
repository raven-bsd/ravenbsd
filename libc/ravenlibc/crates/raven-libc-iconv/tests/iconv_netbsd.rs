// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
// SPDX-License-Identifier: BSD-3-Clause

//! Adapted rows from `netbsd:tests/lib/libc/locale/t_iconv.c`.

use raven_libc_iconv::Iconv;

fn convert(to: &[u8], from: &[u8], input: &[u8]) -> Vec<u8> {
    let mut converter = Iconv::open(to, from).expect("open");
    let mut source = input;
    let mut buffer = [0u8; 64];
    let mut output: &mut [u8] = &mut buffer;
    converter
        .convert(&mut source, &mut output)
        .expect("convert");
    let used = 64 - output.len();
    buffer[..used].to_vec()
}

#[test]
fn utf8_utf16_utf32() {
    assert_eq!(convert(b"UTF-16BE", b"UTF-8", b"A"), &[0x00, 0x41]);
    assert_eq!(
        convert(b"UTF-32BE", b"UTF-8", b"A"),
        &[0x00, 0x00, 0x00, 0x41]
    );
    assert_eq!(convert(b"UTF-8", b"UTF-16BE", &[0x00, 0x41]), b"A");
}

#[test]
fn iso8859_and_cp1252() {
    assert_eq!(
        convert(b"UTF-8", b"ISO-8859-1", &[0xe9]),
        "\u{e9}".as_bytes()
    );
    assert_eq!(
        convert(b"ISO-8859-1", b"UTF-8", "\u{e9}".as_bytes()),
        &[0xe9]
    );
    assert_eq!(convert(b"UTF-8", b"CP1252", &[0x80]), "\u{20ac}".as_bytes());
}

#[test]
fn koi8_r() {
    assert_eq!(convert(b"UTF-8", b"KOI8-R", &[0xc1]), "\u{0430}".as_bytes());
}

#[cfg(feature = "cjk")]
#[test]
fn euc_jp_and_shift_jis() {
    assert_eq!(convert(b"UTF-8", b"EUC-JP", &[0xa4, 0xa2]), "あ".as_bytes());
    assert_eq!(
        convert(b"UTF-8", b"SHIFT_JIS", &[0x82, 0xa0]),
        "あ".as_bytes()
    );
}
