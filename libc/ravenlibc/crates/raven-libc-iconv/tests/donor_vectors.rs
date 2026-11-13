// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
//! Public-API iconv vectors.

use raven_libc_iconv::{Iconv, IconvError};

fn convert(to: &[u8], from: &[u8], input: &[u8]) -> Result<Vec<u8>, IconvError> {
    let mut converter = Iconv::open(to, from)?;
    let mut source = input;
    let mut buffer = vec![0u8; 256];
    let mut output: &mut [u8] = &mut buffer;
    converter.convert(&mut source, &mut output)?;
    let used = 256 - output.len();
    Ok(buffer[..used].to_vec())
}

macro_rules! open_case {
    ($name:ident, $to:expr, $from:expr) => {
        #[test]
        fn $name() {
            assert!(Iconv::open($to, $from).is_ok());
        }
    };
}

open_case!(open_utf8, b"UTF-8", b"UTF-8");
open_case!(open_utf8_alias, b"utf8", b"UTF_8");
open_case!(open_utf16, b"UTF-16", b"UTF-8");
open_case!(open_utf16le, b"UTF-16LE", b"UTF-8");
open_case!(open_utf16be, b"UTF-16BE", b"UTF-8");
open_case!(open_utf32, b"UTF-32", b"UTF-8");
open_case!(open_utf32le, b"UTF-32LE", b"UTF-8");
open_case!(open_utf32be, b"UTF-32BE", b"UTF-8");
open_case!(open_ucs2, b"UCS-2", b"UTF-8");
open_case!(open_ucs2le, b"UCS-2LE", b"UTF-8");
open_case!(open_ucs2be, b"UCS-2BE", b"UTF-8");
open_case!(open_ucs4, b"UCS-4", b"UTF-8");
open_case!(open_ucs4le, b"UCS-4LE", b"UTF-8");
open_case!(open_ucs4be, b"UCS-4BE", b"UTF-8");
open_case!(open_wchar, b"WCHAR_T", b"UTF-8");
open_case!(open_ascii, b"ASCII", b"UTF-8");
open_case!(open_ascii_alias, b"US-ASCII", b"UTF-8");
open_case!(open_ansi, b"ANSI_X3.4-1968", b"UTF-8");
open_case!(open_iso646, b"ISO646-US", b"UTF-8");
open_case!(open_latin1, b"ISO-8859-1", b"UTF-8");
open_case!(open_latin1_alias, b"LATIN1", b"UTF-8");
open_case!(open_l1, b"L1", b"UTF-8");
open_case!(open_iso2, b"ISO-8859-2", b"UTF-8");
open_case!(open_iso15, b"ISO-8859-15", b"UTF-8");
open_case!(open_latin9, b"LATIN9", b"UTF-8");
open_case!(open_tis620, b"TIS-620", b"UTF-8");
open_case!(open_cp1250, b"CP1250", b"UTF-8");
open_case!(open_cp1252, b"CP1252", b"UTF-8");
open_case!(open_windows, b"WINDOWS-1252", b"UTF-8");
open_case!(open_koi8r, b"KOI8-R", b"UTF-8");
open_case!(open_koi8u, b"KOI8-U", b"UTF-8");
open_case!(open_cp437, b"CP437", b"UTF-8");
open_case!(open_cp850, b"CP850", b"UTF-8");
open_case!(open_cp858, b"CP858", b"UTF-8");
open_case!(open_cp866, b"CP866", b"UTF-8");
open_case!(open_ibm1047, b"IBM1047", b"UTF-8");
open_case!(open_empty_name, b"", b"char");
open_case!(open_translit, b"ASCII//TRANSLIT", b"UTF-8");
open_case!(open_ignore, b"ASCII//IGNORE", b"UTF-8");
open_case!(open_both_modes, b"ASCII//TRANSLIT//IGNORE", b"UTF-8");

#[test]
fn unsupported_unknown_and_cjk_targets() {
    assert!(matches!(
        Iconv::open(b"NO-SUCH", b"UTF-8"),
        Err(IconvError::Unsupported)
    ));
    assert!(matches!(
        Iconv::open(b"GB18030", b"UTF-8"),
        Err(IconvError::Unsupported)
    ));
    assert!(matches!(
        Iconv::open(b"BIG5", b"UTF-8"),
        Err(IconvError::Unsupported)
    ));
    assert!(matches!(
        Iconv::open(b"EUC-KR", b"UTF-8"),
        Err(IconvError::Unsupported)
    ));
}

macro_rules! round_trip_case {
    ($name:ident, $charset:expr) => {
        #[test]
        fn $name() {
            for cp in [
                0x00u32, 0x41, 0x7f, 0x80, 0x7ff, 0x800, 0xffff, 0x10000, 0x1f600, 0x10ffff,
            ] {
                let Some(ch) = char::from_u32(cp) else {
                    continue;
                };
                let text = ch.to_string();
                let encoded = convert($charset, b"UTF-8", text.as_bytes()).expect("encode");
                let decoded = convert(b"UTF-8", $charset, &encoded).expect("decode");
                assert_eq!(decoded, text.as_bytes(), "cp={cp:x}");
            }
        }
    };
}

round_trip_case!(round_utf16, b"UTF-16");
round_trip_case!(round_utf16le, b"UTF-16LE");
round_trip_case!(round_utf16be, b"UTF-16BE");
round_trip_case!(round_utf32, b"UTF-32");
round_trip_case!(round_utf32le, b"UTF-32LE");
round_trip_case!(round_utf32be, b"UTF-32BE");
round_trip_case!(round_ucs4, b"UCS-4");
round_trip_case!(round_wchar, b"WCHAR_T");

#[test]
fn utf16_bom_variants() {
    assert_eq!(
        convert(b"UTF-8", b"UTF-16", &[0xfe, 0xff, 0x00, 0x41]).expect("bom"),
        b"A"
    );
    assert_eq!(
        convert(b"UTF-8", b"UTF-16", &[0xff, 0xfe, 0x41, 0x00]).expect("le bom"),
        b"A"
    );
    assert_eq!(
        convert(b"UTF-8", b"UTF-16", &[0x00, 0x41]).expect("be default"),
        b"A"
    );
}

#[test]
fn utf16_target_has_no_bom() {
    assert_eq!(
        convert(b"UTF-16", b"UTF-8", b"A").expect("encode"),
        &[0x00, 0x41]
    );
    assert_eq!(
        convert(b"UTF-16LE", b"UTF-8", b"A").expect("encode le"),
        &[0x41, 0x00]
    );
}

#[test]
fn ascii_strict_translit_ignore() {
    let mut strict = Iconv::open(b"ASCII", b"UTF-8").expect("strict");
    let mut input = "\u{20ac}".as_bytes();
    let mut buffer = [0u8; 8];
    let mut output: &mut [u8] = &mut buffer;
    assert_eq!(
        strict.convert(&mut input, &mut output),
        Err(IconvError::IllegalSequence)
    );
    assert_eq!(strict.reset(), ());
    let mut input = "A\u{20ac}".as_bytes();
    let mut buffer = [0u8; 8];
    let mut output: &mut [u8] = &mut buffer;
    let mut translit = Iconv::open(b"ASCII//TRANSLIT", b"UTF-8").expect("translit");
    assert_eq!(translit.convert(&mut input, &mut output), Ok(1));
    assert_eq!(&buffer[..2], b"A?");
    let mut input = "A\u{20ac}".as_bytes();
    let mut buffer = [0u8; 8];
    let mut output: &mut [u8] = &mut buffer;
    let mut ignore = Iconv::open(b"ASCII//IGNORE", b"UTF-8").expect("ignore");
    assert_eq!(
        ignore.convert(&mut input, &mut output),
        Err(IconvError::IllegalSequence)
    );
    assert!(input.is_empty());
}

#[test]
fn error_positions_and_too_big() {
    let mut converter = Iconv::open(b"UTF-8", b"UTF-8").expect("open");
    let mut input: &[u8] = &[0xe2, 0x82];
    let mut buffer = [0u8; 8];
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
    let mut converter = Iconv::open(b"UTF-16", b"UTF-8").expect("open");
    let mut input: &[u8] = b"A";
    let mut empty: &mut [u8] = &mut [];
    assert_eq!(
        converter.convert(&mut input, &mut empty),
        Err(IconvError::TooBig)
    );
    assert_eq!(input, b"A");
}

#[test]
fn surrogates_and_ucs2_replacement() {
    assert_eq!(
        convert(b"UTF-8", b"UTF-16LE", &[0x00, 0xd8, 0x00, 0x00]),
        Err(IconvError::IllegalSequence)
    );
    assert_eq!(
        convert(b"UTF-8", b"UTF-32LE", &[0x00, 0xd8, 0x00, 0x00]),
        Err(IconvError::IllegalSequence)
    );
    let encoded = convert(b"UCS-2", b"UTF-8", "\u{1f600}".as_bytes()).expect("ucs2");
    assert_eq!(encoded, &[0xff, 0xfd]);
}

#[test]
fn latin1_upper_half() {
    assert_eq!(
        convert(b"UTF-8", b"ISO-8859-1", &[0xe9]).expect("decode"),
        "\u{e9}".as_bytes()
    );
    assert_eq!(
        convert(b"ISO-8859-1", b"UTF-8", "\u{e9}".as_bytes()).expect("encode"),
        &[0xe9]
    );
}

#[test]
fn stateful_flags() {
    assert!(
        Iconv::open(b"UTF-8", b"UTF-16")
            .expect("open")
            .is_stateful()
    );
    assert!(
        Iconv::open(b"UTF-8", b"UTF-32")
            .expect("open")
            .is_stateful()
    );
    assert!(!Iconv::open(b"UTF-8", b"UTF-8").expect("open").is_stateful());
}

#[cfg(feature = "cjk")]
#[test]
fn cjk_euc_jp_round_trip() {
    let encoded = convert(b"EUC-JP", b"UTF-8", "あ".as_bytes()).expect("encode");
    assert_eq!(encoded, &[0xa4, 0xa2]);
    assert_eq!(
        convert(b"UTF-8", b"EUC-JP", &[0xa4, 0xa2]).expect("decode"),
        "あ".as_bytes()
    );
}

#[cfg(feature = "cjk")]
#[test]
fn cjk_shift_jis_round_trip() {
    let encoded = convert(b"SHIFT_JIS", b"UTF-8", "あ".as_bytes()).expect("encode");
    assert_eq!(encoded, &[0x82, 0xa0]);
    assert_eq!(
        convert(b"UTF-8", b"SHIFT_JIS", &[0x82, 0xa0]).expect("decode"),
        "あ".as_bytes()
    );
}

#[cfg(feature = "cjk")]
#[test]
fn cjk_gb_big5_ksc_decode() {
    assert_eq!(
        convert(b"UTF-8", b"GB18030", &[0xb0, 0xa1]).expect("gb"),
        "啊".as_bytes()
    );
    assert_eq!(
        convert(b"UTF-8", b"BIG5", &[0xa4, 0x40]).expect("big5"),
        "一".as_bytes()
    );
    assert_eq!(
        convert(b"UTF-8", b"EUC-KR", &[0xb0, 0xa1]).expect("ksc"),
        "가".as_bytes()
    );
}

#[cfg(feature = "cjk")]
#[test]
fn cjk_forbidden_targets() {
    for name in [b"GB18030".as_slice(), b"GBK", b"GB2312", b"BIG5", b"EUC-KR"] {
        assert!(matches!(
            Iconv::open(name, b"UTF-8"),
            Err(IconvError::Unsupported)
        ));
    }
    for name in [b"EUC-JP".as_slice(), b"SHIFT_JIS", b"ISO-2022-JP"] {
        assert!(Iconv::open(name, b"UTF-8").is_ok());
    }
}
