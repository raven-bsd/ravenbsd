// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
//! Broad UTF and alias matrix.

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

macro_rules! cp_case {
    ($name:ident, $cp:expr) => {
        #[test]
        fn $name() {
            let ch = char::from_u32($cp).expect("scalar");
            let text = ch.to_string();
            for target in [
                b"UTF-16LE".as_slice(),
                b"UTF-16BE",
                b"UTF-32LE",
                b"UTF-32BE",
                b"WCHAR_T",
            ] {
                let encoded = convert(target, b"UTF-8", text.as_bytes());
                let decoded = convert(b"UTF-8", target, &encoded);
                assert_eq!(decoded, text.as_bytes(), "cp={:x}", $cp);
            }
        }
    };
}

cp_case!(cp_0000, 0x00);
cp_case!(cp_0001, 0x01);
cp_case!(cp_0002, 0x02);
cp_case!(cp_0003, 0x03);
cp_case!(cp_0004, 0x04);
cp_case!(cp_0005, 0x05);
cp_case!(cp_0006, 0x06);
cp_case!(cp_0007, 0x07);
cp_case!(cp_0008, 0x08);
cp_case!(cp_0009, 0x09);
cp_case!(cp_000a, 0x0a);
cp_case!(cp_000b, 0x0b);
cp_case!(cp_000c, 0x0c);
cp_case!(cp_000d, 0x0d);
cp_case!(cp_000e, 0x0e);
cp_case!(cp_000f, 0x0f);
cp_case!(cp_0010, 0x10);
cp_case!(cp_0011, 0x11);
cp_case!(cp_0012, 0x12);
cp_case!(cp_0013, 0x13);
cp_case!(cp_0014, 0x14);
cp_case!(cp_0015, 0x15);
cp_case!(cp_0016, 0x16);
cp_case!(cp_0017, 0x17);
cp_case!(cp_0018, 0x18);
cp_case!(cp_0019, 0x19);
cp_case!(cp_001a, 0x1a);
cp_case!(cp_001b, 0x1b);
cp_case!(cp_001c, 0x1c);
cp_case!(cp_001d, 0x1d);
cp_case!(cp_001e, 0x1e);
cp_case!(cp_001f, 0x1f);
cp_case!(cp_0020, 0x20);
cp_case!(cp_007e, 0x7e);
cp_case!(cp_007f, 0x7f);
cp_case!(cp_0080, 0x80);
cp_case!(cp_00e9, 0xe9);
cp_case!(cp_0100, 0x100);
cp_case!(cp_07ff, 0x7ff);
cp_case!(cp_0800, 0x800);
cp_case!(cp_20ac, 0x20ac);
cp_case!(cp_4e00, 0x4e00);
cp_case!(cp_ffff, 0xffff);
cp_case!(cp_10000, 0x10000);
cp_case!(cp_1f600, 0x1f600);
cp_case!(cp_10fffe, 0x10fffe);
cp_case!(cp_10ffff, 0x10ffff);
cp_case!(cp_d7ff, 0xd7ff);
cp_case!(cp_e000, 0xe000);
cp_case!(cp_fffd, 0xfffd);

macro_rules! ascii_case {
    ($name:ident, $charset:expr) => {
        #[test]
        fn $name() {
            assert_eq!(convert($charset, b"UTF-8", b"A"), b"A");
            assert_eq!(convert(b"UTF-8", $charset, b"A"), b"A");
        }
    };
}

ascii_case!(ascii_ascii, b"ASCII");
ascii_case!(ascii_iso1, b"ISO-8859-1");
ascii_case!(ascii_iso2, b"ISO-8859-2");
ascii_case!(ascii_iso3, b"ISO-8859-3");
ascii_case!(ascii_iso4, b"ISO-8859-4");
ascii_case!(ascii_iso5, b"ISO-8859-5");
ascii_case!(ascii_iso6, b"ISO-8859-6");
ascii_case!(ascii_iso7, b"ISO-8859-7");
ascii_case!(ascii_iso8, b"ISO-8859-8");
ascii_case!(ascii_iso9, b"ISO-8859-9");
ascii_case!(ascii_iso10, b"ISO-8859-10");
ascii_case!(ascii_iso11, b"ISO-8859-11");
ascii_case!(ascii_iso13, b"ISO-8859-13");
ascii_case!(ascii_iso14, b"ISO-8859-14");
ascii_case!(ascii_iso15, b"ISO-8859-15");
ascii_case!(ascii_iso16, b"ISO-8859-16");
ascii_case!(ascii_cp1250, b"CP1250");
ascii_case!(ascii_cp1252, b"CP1252");
ascii_case!(ascii_koi8r, b"KOI8-R");
ascii_case!(ascii_cp437, b"CP437");
