// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
//! CJK feature vectors.

#![cfg(feature = "cjk")]

use raven_libc_iconv::{Iconv, IconvError};

fn decode(charset: &[u8], input: &[u8]) -> Result<Vec<u8>, IconvError> {
    let mut converter = Iconv::open(b"UTF-8", charset)?;
    let mut source = input;
    let mut buffer = [0u8; 64];
    let mut output: &mut [u8] = &mut buffer;
    converter.convert(&mut source, &mut output)?;
    let used = 64 - output.len();
    Ok(buffer[..used].to_vec())
}

fn encode(charset: &[u8], input: &[u8]) -> Result<Vec<u8>, IconvError> {
    let mut converter = Iconv::open(charset, b"UTF-8")?;
    let mut source = input;
    let mut buffer = [0u8; 64];
    let mut output: &mut [u8] = &mut buffer;
    converter.convert(&mut source, &mut output)?;
    let used = 64 - output.len();
    Ok(buffer[..used].to_vec())
}

macro_rules! decode_ascii_case {
    ($name:ident, $charset:expr) => {
        #[test]
        fn $name() {
            assert_eq!(decode($charset, b"A").unwrap(), b"A");
        }
    };
}

decode_ascii_case!(cjk_eucjp_ascii, b"EUC-JP");
decode_ascii_case!(cjk_sjis_ascii, b"SHIFT_JIS");
decode_ascii_case!(cjk_iso2022_ascii, b"ISO-2022-JP");
decode_ascii_case!(cjk_gb18030_ascii, b"GB18030");
decode_ascii_case!(cjk_gbk_ascii, b"GBK");
decode_ascii_case!(cjk_gb2312_ascii, b"GB2312");
decode_ascii_case!(cjk_big5_ascii, b"BIG5");
decode_ascii_case!(cjk_euckr_ascii, b"EUC-KR");
decode_ascii_case!(cjk_cp932_ascii, b"CP932");
decode_ascii_case!(cjk_cp936_ascii, b"CP936");
decode_ascii_case!(cjk_cp950_ascii, b"CP950");
decode_ascii_case!(cjk_cp949_ascii, b"CP949");
decode_ascii_case!(cjk_big5hkscs_ascii, b"BIG5-HKSCS");
decode_ascii_case!(cjk_windows936_ascii, b"WINDOWS-936");

macro_rules! encode_ascii_case {
    ($name:ident, $charset:expr) => {
        #[test]
        fn $name() {
            assert_eq!(encode($charset, b"A").unwrap(), b"A");
        }
    };
}

encode_ascii_case!(cjk_eucjp_encode_ascii, b"EUC-JP");
encode_ascii_case!(cjk_sjis_encode_ascii, b"SHIFT_JIS");
encode_ascii_case!(cjk_iso2022_encode_ascii, b"ISO-2022-JP");

macro_rules! reset_case {
    ($name:ident, $charset:expr) => {
        #[test]
        fn $name() {
            let mut converter = Iconv::open(b"UTF-8", $charset).expect("open");
            converter.reset();
            assert_eq!(decode($charset, b"A").unwrap(), b"A");
        }
    };
}

reset_case!(cjk_eucjp_reset, b"EUC-JP");
reset_case!(cjk_sjis_reset, b"SHIFT_JIS");
reset_case!(cjk_iso2022_reset, b"ISO-2022-JP");
reset_case!(cjk_gb18030_reset, b"GB18030");
reset_case!(cjk_big5_reset, b"BIG5");
reset_case!(cjk_euckr_reset, b"EUC-KR");

macro_rules! jis_case {
    ($name:ident, $row:expr, $column:expr) => {
        #[test]
        fn $name() {
            let bytes = [0xa1 + $row, 0xa1 + $column];
            let text = decode(b"EUC-JP", &bytes).expect("decode");
            let encoded = encode(b"EUC-JP", &text).expect("encode");
            assert_eq!(encoded, bytes);
        }
    };
}

jis_case!(jis_00, 0, 0);
jis_case!(jis_01, 0, 1);
jis_case!(jis_02, 0, 2);
jis_case!(jis_03, 0, 3);
jis_case!(jis_04, 0, 4);
jis_case!(jis_05, 0, 5);
jis_case!(jis_06, 0, 6);
jis_case!(jis_07, 0, 7);
jis_case!(jis_08, 0, 8);
jis_case!(jis_09, 0, 9);
jis_case!(jis_10, 1, 0);
jis_case!(jis_11, 1, 1);
jis_case!(jis_12, 1, 2);
jis_case!(jis_13, 1, 3);
jis_case!(jis_14, 1, 4);
jis_case!(jis_15, 1, 5);

#[test]
fn cjk_shift_jis_vectors() {
    assert_eq!(
        decode(b"SHIFT_JIS", &[0x82, 0xa0]).unwrap(),
        "あ".as_bytes()
    );
    assert_eq!(
        encode(b"SHIFT_JIS", "あ".as_bytes()).unwrap(),
        &[0x82, 0xa0]
    );
}

#[test]
fn cjk_euc_jp_vectors() {
    assert_eq!(decode(b"EUC-JP", &[0xa4, 0xa2]).unwrap(), "あ".as_bytes());
    assert_eq!(encode(b"EUC-JP", "あ".as_bytes()).unwrap(), &[0xa4, 0xa2]);
}

#[test]
fn cjk_gb18030_four_byte_vector() {
    assert_eq!(
        decode(b"GB18030", &[0x90, 0x30, 0x81, 0x30]).unwrap(),
        "\u{10000}".as_bytes()
    );
}

#[test]
fn cjk_big5_vector() {
    assert_eq!(decode(b"BIG5", &[0xa4, 0x40]).unwrap(), "一".as_bytes());
}

#[test]
fn cjk_euc_kr_vector() {
    assert_eq!(decode(b"EUC-KR", &[0xb0, 0xa1]).unwrap(), "가".as_bytes());
}
