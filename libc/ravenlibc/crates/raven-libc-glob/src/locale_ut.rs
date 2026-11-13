// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the locale seam.
// Test-only code is exempt from the production lints (17-crate-layout 9.3).
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use super::*;

#[test]
fn byte_locale_is_one_byte_per_unit() {
    let loc = ByteLocale;
    assert_eq!(loc.mb_cur_max(), 1);
    assert_eq!(
        loc.mbtowc(b"\xc3\xa9"),
        MbResult::Char { wc: 0xdfc3, len: 1 }
    );
    assert_eq!(loc.mbtowc(b""), MbResult::Invalid);
}

#[test]
fn utf8_locale_decodes_one_to_four_bytes() {
    let loc = Utf8Locale;
    assert_eq!(loc.mb_cur_max(), 4);
    assert_eq!(
        loc.mbtowc(b"a"),
        MbResult::Char {
            wc: u32::from(b'a'),
            len: 1
        }
    );
    assert_eq!(
        loc.mbtowc("é".as_bytes()),
        MbResult::Char { wc: 0xe9, len: 2 }
    );
    assert_eq!(
        loc.mbtowc("€".as_bytes()),
        MbResult::Char { wc: 0x20ac, len: 3 }
    );
    assert_eq!(
        loc.mbtowc("😀".as_bytes()),
        MbResult::Char {
            wc: 0x1f600,
            len: 4
        }
    );
    assert_eq!(loc.mbtowc(b"\x80"), MbResult::Invalid);
    assert_eq!(loc.mbtowc(b"\xc3"), MbResult::Invalid);
}

#[test]
fn wctype_names_and_classes() {
    let loc = Utf8Locale;
    let Some(alpha) = loc.wctype(b"alpha") else {
        panic!("alpha class missing");
    };
    assert!(loc.iswctype(u32::from(b'a'), alpha));
    assert!(loc.iswctype(0xe9, alpha));
    assert!(!loc.iswctype(u32::from(b'1'), alpha));
    assert_eq!(loc.wctype(b"no-such-class"), None);
}

#[test]
fn case_mapping_is_simple() {
    let loc = Utf8Locale;
    assert_eq!(loc.towupper(u32::from(b'a')), u32::from(b'A'));
    assert_eq!(loc.towlower(u32::from(b'A')), u32::from(b'a'));
    assert_eq!(loc.towupper(0xe9), 0xc9);
    assert_eq!(loc.towlower(0xc9), 0xe9);
}

#[test]
fn encode_round_trips_and_falls_back() {
    let utf8 = Utf8Locale;
    let (bytes, len) = encode(&utf8, 0xe9);
    assert_eq!(&bytes[..len], "é".as_bytes());
    let (bytes, len) = encode(&utf8, u32::from(b'x'));
    assert_eq!(&bytes[..len], b"x");
    let byte = ByteLocale;
    assert_eq!(byte.mbtowc(b"\xff"), MbResult::Char { wc: 0xdfff, len: 1 });
    let (bytes, len) = encode(&byte, 0xdfe9);
    assert_eq!(&bytes[..len], b"\xe9");
    assert_eq!(encode(&byte, 0xe9).1, 0);
    assert!(mode_of(&byte) == MbMode::Byte);
    assert!(mode_of(&utf8) == MbMode::Utf8);
}
