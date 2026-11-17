// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the iconv conversion library.

use super::*;

#[test]
fn resolves_supported_names() {
    assert_eq!(parse_encoding("UTF-8"), Some(Encoding::Utf8));
    assert_eq!(parse_encoding("utf8"), Some(Encoding::Utf8));
    assert_eq!(parse_encoding("US-ASCII"), Some(Encoding::Ascii));
    assert_eq!(parse_encoding("ANSI_X3.4-1968"), Some(Encoding::Ascii));
    assert_eq!(parse_encoding("latin1"), Some(Encoding::Latin1));
    assert_eq!(parse_encoding("ISO-8859-1"), Some(Encoding::Latin1));
    assert_eq!(parse_encoding("UTF-16"), None);
}

#[test]
fn converts_utf8_to_latin1_and_back() {
    let utf8 = "café\n";
    let latin1 = convert(utf8.as_bytes(), Encoding::Utf8, Encoding::Latin1).expect("to latin1");
    assert_eq!(latin1, b"caf\xe9\n");
    let back = convert(&latin1, Encoding::Latin1, Encoding::Utf8).expect("to utf8");
    assert_eq!(back, utf8.as_bytes());
}

#[test]
fn converts_ascii_and_rejects_non_ascii() {
    assert_eq!(
        convert(b"plain\n", Encoding::Ascii, Encoding::Utf8).expect("ascii"),
        b"plain\n"
    );
    assert!(convert(&[0x80], Encoding::Ascii, Encoding::Utf8).is_err());
    assert!(convert("é".as_bytes(), Encoding::Utf8, Encoding::Ascii).is_err());
}

#[test]
fn rejects_invalid_utf8() {
    assert!(convert(&[0xff], Encoding::Utf8, Encoding::Utf8).is_err());
    assert!(convert(&[0xc3], Encoding::Utf8, Encoding::Utf8).is_err());
    assert!(convert(&[0xc3, 0x28], Encoding::Utf8, Encoding::Utf8).is_err());
    assert!(convert(&[0xc0, 0x80], Encoding::Utf8, Encoding::Utf8).is_err());
}

#[test]
fn latin1_rejects_code_points_above_ff() {
    assert!(convert("€".as_bytes(), Encoding::Utf8, Encoding::Latin1).is_err());
    assert_eq!(
        convert(&[0xff], Encoding::Latin1, Encoding::Utf8).expect("ff"),
        "ÿ".as_bytes()
    );
}
