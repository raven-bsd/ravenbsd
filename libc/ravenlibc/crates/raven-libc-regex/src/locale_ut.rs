// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of the two locale objects.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;

fn utf8(input: &[u8]) -> MbResult {
    let mut state = MbState::INITIAL;
    Utf8Locale.mbrtowc(input, &mut state)
}

#[test]
fn utf8_decodes_one_to_four_bytes() {
    assert_eq!(utf8(b"A"), MbResult::Ok(0x41, 1));
    assert_eq!(utf8("\u{e9}".as_bytes()), MbResult::Ok(0xe9, 2));
    assert_eq!(utf8("\u{20ac}".as_bytes()), MbResult::Ok(0x20ac, 3));
    assert_eq!(utf8("\u{1f600}".as_bytes()), MbResult::Ok(0x1_f600, 4));
}

#[test]
fn utf8_rejects_overlong_and_surrogate_and_too_large() {
    assert_eq!(utf8(&[0xc0, 0x80]), MbResult::Invalid);
    assert_eq!(utf8(&[0xed, 0xa0, 0x80]), MbResult::Invalid);
    assert_eq!(utf8(&[0xf4, 0x90, 0x80, 0x80]), MbResult::Invalid);
    assert_eq!(utf8(&[0x80]), MbResult::Invalid);
    assert_eq!(utf8(&[0xff]), MbResult::Invalid);
}

#[test]
fn utf8_truncated_sequence_is_incomplete() {
    assert_eq!(utf8(&[0xe2, 0x82]), MbResult::Incomplete);
    assert_eq!(utf8(&[]), MbResult::Incomplete);
}

#[test]
fn byte_locale_passes_every_byte_through() {
    for byte in 0u16..=255 {
        let input = [byte as u8];
        let mut state = MbState::INITIAL;
        assert_eq!(
            ByteLocale.mbrtowc(&input, &mut state),
            MbResult::Ok(u32::from(byte as u8), 1)
        );
    }
    let mut state = MbState::INITIAL;
    assert_eq!(ByteLocale.mbrtowc(&[], &mut state), MbResult::Incomplete);
}

#[test]
fn mb_cur_max_is_one_and_four() {
    assert_eq!(ByteLocale.mb_cur_max(), 1);
    assert_eq!(Utf8Locale.mb_cur_max(), 4);
}

#[test]
fn class_names_map_to_the_twelve_types() {
    for (name, class) in [
        (&b"alnum"[..], Wctype::Alnum),
        (&b"alpha"[..], Wctype::Alpha),
        (&b"blank"[..], Wctype::Blank),
        (&b"cntrl"[..], Wctype::Cntrl),
        (&b"digit"[..], Wctype::Digit),
        (&b"graph"[..], Wctype::Graph),
        (&b"lower"[..], Wctype::Lower),
        (&b"print"[..], Wctype::Print),
        (&b"punct"[..], Wctype::Punct),
        (&b"space"[..], Wctype::Space),
        (&b"upper"[..], Wctype::Upper),
        (&b"xdigit"[..], Wctype::Xdigit),
    ] {
        assert_eq!(Utf8Locale.wctype(name), Some(class));
        assert_eq!(ByteLocale.wctype(name), Some(class));
        assert_eq!(Wctype::from_code(class.code()), Some(class));
    }
    assert_eq!(Utf8Locale.wctype(b"notdef"), None);
}

#[test]
fn classification_and_case_mapping_go_through_ctype() {
    assert!(Utf8Locale.iswalpha(u32::from('A')));
    assert!(Utf8Locale.iswalpha(0xe9));
    assert!(!Utf8Locale.iswalpha(u32::from('0')));
    assert!(Utf8Locale.iswupper(u32::from('A')));
    assert!(Utf8Locale.iswlower(u32::from('a')));
    assert_eq!(Utf8Locale.towlower(u32::from('A')), u32::from('a'));
    assert_eq!(Utf8Locale.towupper(u32::from('a')), u32::from('A'));
    assert_eq!(Utf8Locale.towlower(0xc9), 0xe9);
    assert!(Utf8Locale.iswctype(u32::from('7'), Wctype::Digit));
}

#[test]
fn byte_locale_classification_uses_the_wide_tables() {
    assert!(ByteLocale.iswalpha(u32::from('a')));
    assert!(ByteLocale.iswalpha(0xe9));
    assert_eq!(ByteLocale.towupper(0xe9), 0xc9);
}

#[test]
fn utf8_nul_is_a_one_byte_character() {
    assert_eq!(utf8(&[0]), MbResult::Ok(0, 1));
}

#[test]
fn utf8_two_byte_boundaries() {
    assert_eq!(utf8(&[0xc2, 0x80]), MbResult::Ok(0x80, 2));
    assert_eq!(utf8(&[0xdf, 0xbf]), MbResult::Ok(0x7ff, 2));
}

#[test]
fn utf8_three_byte_boundaries() {
    assert_eq!(utf8(&[0xe0, 0xa0, 0x80]), MbResult::Ok(0x800, 3));
    assert_eq!(utf8(&[0xef, 0xbf, 0xbf]), MbResult::Ok(0xffff, 3));
}

#[test]
fn utf8_four_byte_boundaries() {
    assert_eq!(utf8(&[0xf0, 0x90, 0x80, 0x80]), MbResult::Ok(0x1_0000, 4));
    assert_eq!(utf8(&[0xf4, 0x8f, 0xbf, 0xbf]), MbResult::Ok(0x10_ffff, 4));
}

#[test]
fn byte_locale_nul_and_high_byte() {
    let mut state = MbState::INITIAL;
    assert_eq!(ByteLocale.mbrtowc(&[0], &mut state), MbResult::Ok(0, 1));
    assert_eq!(
        ByteLocale.mbrtowc(&[0xff], &mut state),
        MbResult::Ok(0xff, 1)
    );
}

#[test]
fn class_names_are_case_sensitive() {
    assert_eq!(Utf8Locale.wctype(b"Alpha"), None);
    assert_eq!(Utf8Locale.wctype(b"ALPHA"), None);
    assert_eq!(ByteLocale.wctype(b"alpha\0"), None);
}

#[test]
fn unknown_class_codes_are_false() {
    assert!(!Utf8Locale.iswctype(u32::from('a'), Wctype::Digit));
    assert!(!Utf8Locale.iswctype(u32::from('7'), Wctype::Alpha));
}

#[test]
fn case_mapping_is_identity_for_non_letters() {
    assert_eq!(Utf8Locale.towlower(u32::from('1')), u32::from('1'));
    assert_eq!(Utf8Locale.towupper(u32::from('-')), u32::from('-'));
}

#[test]
fn byte_locale_iswctype_matches_the_wide_tables() {
    assert!(ByteLocale.iswctype(u32::from(' '), Wctype::Space));
    assert!(!ByteLocale.iswctype(u32::from('x'), Wctype::Space));
    assert!(ByteLocale.iswctype(u32::from('f'), Wctype::Xdigit));
}
