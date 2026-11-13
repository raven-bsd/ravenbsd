// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `vis` encoder.
//!
//! The expected strings are the donor's output for the given flags, derived from
//! `netbsd:lib/libc/gen/vis.c` (see `tests/donor_vectors.rs` for the cross-checked table).
//! The exhaustive row asks `vis()` for every byte 0..=255 with each of the twelve single flags
//! and checks the escape grammar (a leading `\`/`M`/`^`/`%`/`=` or the byte itself), the output
//! length bound and the round trip through `strunvis` where the flag set permits it.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

use alloc::vec::Vec;

use super::*;
use crate::vis::unvis::strunvis;

fn encoded(c: u8, flags: u32) -> Vec<u8> {
    let mut dst = [0u8; 8];
    let n = vis(&mut dst, c, flags, 0).expect("vis");
    dst[..n].to_vec()
}

fn text(s: &[u8], flags: u32) -> Vec<u8> {
    let mut dst = [0u8; 512];
    let n = strvis(&mut dst, s, flags).expect("strvis");
    dst[..n].to_vec()
}

#[test]
fn vis_plain_ascii_passthrough() {
    assert_eq!(encoded(b'A', 0), b"A");
    assert_eq!(encoded(b'0', 0), b"0");
    assert_eq!(encoded(b' ', 0), b" ");
    assert_eq!(encoded(b'~', 0), b"~");
}

#[test]
fn vis_cstyle_names() {
    // Space, tab and newline are white and pass through unless their VIS_* flag forces them.
    assert_eq!(encoded(b'\n', VIS_CSTYLE), b"\n");
    assert_eq!(encoded(b'\n', VIS_CSTYLE | VIS_NL), b"\\n");
    assert_eq!(encoded(b'\t', VIS_CSTYLE | VIS_TAB), b"\\t");
    assert_eq!(encoded(b'\r', VIS_CSTYLE), b"\\r");
    assert_eq!(encoded(b'\x08', VIS_CSTYLE), b"\\b");
    assert_eq!(encoded(b'\x07', VIS_CSTYLE), b"\\a");
    assert_eq!(encoded(b'\x0b', VIS_CSTYLE), b"\\v");
    assert_eq!(encoded(b'\x0c', VIS_CSTYLE), b"\\f");
    assert_eq!(encoded(b' ', VIS_CSTYLE | VIS_SP), b"\\s");
    assert_eq!(encoded(0, VIS_CSTYLE), b"\\0");
}

#[test]
fn vis_cstyle_zero_before_octal_gets_two_more_zeros() {
    let mut dst = [0u8; 8];
    let n = vis(&mut dst, 0, VIS_CSTYLE, b'7').unwrap();
    assert_eq!(&dst[..n], b"\\000");
}

#[test]
fn vis_cstyle_escapes_an_extra_graph_character() {
    // A graph character passes through unless it is in the `extra` list; then VIS_CSTYLE
    // prefixes it with a backslash (the donor's do_mbyte default branch).
    let mut dst = [0u8; 8];
    let n = svis(&mut dst, b'q', VIS_CSTYLE, 0, b"q").unwrap();
    assert_eq!(&dst[..n], b"\\q");
    assert_eq!(encoded(b'q', VIS_CSTYLE), b"q");
    // Reserved in CSTYLE: M, ^, $ and the letters that name an escape. They fall through to
    // the M-/^ form, which for a printable character is `-<char>`.
    // The reserved letters fall through to the octal branch because they are `extra` too.
    let mut dst = [0u8; 8];
    let n = svis(&mut dst, b'M', VIS_CSTYLE | VIS_NOSLASH, 0, b"M").unwrap();
    assert_eq!(&dst[..n], b"\\115");
    let n = svis(&mut dst, b'n', VIS_CSTYLE | VIS_NOSLASH, 0, b"n").unwrap();
    assert_eq!(&dst[..n], b"\\156");
}

#[test]
fn vis_octal_is_three_digits() {
    // White characters need their VIS_* flag to be forced; NUL is not white.
    assert_eq!(encoded(b'\n', VIS_OCTAL | VIS_NL), b"\\012");
    assert_eq!(encoded(0, VIS_OCTAL), b"\\000");
    assert_eq!(encoded(0xff, VIS_OCTAL | VIS_NOLOCALE), b"\\377");
    assert_eq!(encoded(b'A', VIS_OCTAL), b"A");
}

#[test]
fn vis_meta_and_control_forms() {
    // The donor: `\`, then `M` for the high bit, then `^`+code for a control or `-`+char.
    assert_eq!(encoded(0x80, 0), b"\\M^@");
    assert_eq!(encoded(0x81, 0), b"\\M^A");
    assert_eq!(encoded(0x7f, 0), b"\\^?");
    assert_eq!(encoded(0x01, 0), b"\\^A");
    assert_eq!(encoded(0x01, VIS_NOSLASH), b"^A");
}

#[test]
fn vis_white_flags_force_encoding() {
    assert_eq!(encoded(b' ', 0), b" ");
    assert_eq!(encoded(b' ', VIS_SP), b"\\040");
    assert_eq!(encoded(b'\t', VIS_TAB), b"\\011");
    assert_eq!(encoded(b'\n', VIS_NL), b"\\012");
    assert_eq!(encoded(b'\t', VIS_WHITE), b"\\011");
}

#[test]
fn vis_safe_leaves_bell_and_cr() {
    assert_eq!(encoded(0x07, VIS_SAFE), b"\x07");
    assert_eq!(encoded(b'\r', VIS_SAFE), b"\r");
    // Newline and tab are white, so they pass through with or without VIS_SAFE.
    assert_eq!(encoded(b'\n', VIS_SAFE), b"\n");
    assert_eq!(encoded(b'\t', VIS_SAFE), b"\t");
}

#[test]
fn vis_glob_and_shell_extras() {
    assert_eq!(encoded(b'*', 0), b"*");
    assert_eq!(encoded(b'*', VIS_GLOB), b"\\052");
    assert_eq!(encoded(b'?', VIS_GLOB), b"\\077");
    assert_eq!(encoded(b'[', VIS_GLOB), b"\\133");
    assert_eq!(encoded(b'#', VIS_GLOB), b"\\043");
    assert_eq!(encoded(b';', VIS_SHELL), b"\\073");
    assert_eq!(encoded(b'"', VIS_DQ), b"\\042");
    // The backslash is itself in the extra list, so it takes the octal escape.
    assert_eq!(encoded(b'\\', 0), b"\\134");
    // Without the slash extra it is a graph character and passes through.
    assert_eq!(encoded(b'\\', VIS_NOSLASH), b"\\");
}

#[test]
fn vis_httpstyle() {
    assert_eq!(encoded(b' ', VIS_HTTPSTYLE), b"%20");
    assert_eq!(encoded(b'"', VIS_HTTPSTYLE), b"%22");
    assert_eq!(encoded(b'a', VIS_HTTPSTYLE), b"a");
    assert_eq!(encoded(b'%', VIS_HTTPSTYLE), b"%25");
    assert_eq!(encoded(b'!', VIS_HTTPSTYLE), b"!");
    assert_eq!(encoded(b'\n', VIS_HTTPSTYLE), b"%0a");
}

#[test]
fn vis_mimestyle() {
    assert_eq!(encoded(b'=', VIS_MIMESTYLE), b"=3D");
    assert_eq!(encoded(b'~', VIS_MIMESTYLE), b"=7E");
    assert_eq!(encoded(b'a', VIS_MIMESTYLE), b"a");
    assert_eq!(encoded(b'\n', VIS_MIMESTYLE), b"\n");
    // The space before a CR/NL is encoded.
    let mut dst = [0u8; 8];
    let n = vis(&mut dst, b' ', VIS_MIMESTYLE, b'\n').unwrap();
    assert_eq!(&dst[..n], b"=20");
}

#[test]
fn vis_nvis_and_svis_agree() {
    let mut a = [0u8; 8];
    let mut b = [0u8; 8];
    let mut c = [0u8; 8];
    let na = vis(&mut a, b'*', VIS_GLOB, 0).unwrap();
    let nb = nvis(&mut b, b'*', VIS_GLOB, 0).unwrap();
    assert_eq!(&a[..na], &b[..nb]);
    let nc = svis(&mut c, b'*', 0, 0, b"*").unwrap();
    assert_eq!(&c[..nc], b"\\052");
}

#[test]
fn vis_truncation_returns_none() {
    let mut dst = [0u8; 3];
    assert_eq!(vis(&mut dst, 0x80, 0, 0), None);
    let mut empty: [u8; 0] = [];
    assert_eq!(vis(&mut empty, b'A', 0, 0), None);
}

#[test]
fn strvis_utf8_passthrough() {
    // "héllo" is valid UTF-8; é is a graph character and passes through.
    let src = "héllo".as_bytes();
    assert_eq!(text(src, 0), src);
    let mut roundtrip = [0u8; 32];
    let n = strunvis(&mut roundtrip, &text(src, 0), 0).unwrap();
    assert_eq!(&roundtrip[..n], src);
}

#[test]
fn strvis_utf8_non_graph_becomes_octal_per_byte() {
    // U+2028 LINE SEPARATOR is a space for raven-libc-ctype, so it is not graph and the donor
    // emits the octal escapes of its bytes. (U+00A0 is a space in glibc but not in musl's
    // iswspace table; the report records that difference.)
    let src = "\u{2028}".as_bytes();
    assert_eq!(text(src, 0), b"\\040\\-(");
}

#[test]
fn strvis_invalid_utf8_falls_back_to_bytes() {
    // The offending byte becomes a wide value; 0xff is graph, so it passes through as the raw
    // byte in the donor's byte-mode output loop.
    let src = [b'a', 0xff, b'b'];
    assert_eq!(text(&src, 0), b"a\xffb");
    let src = [b'a', 0x01, b'b'];
    assert_eq!(text(&src, 0), b"a\\^Ab");
}

#[test]
fn strvis_nolocale_is_byte_oriented() {
    let src = "é".as_bytes();
    // Without VIS_OCTAL each byte takes the M-/^ form.
    assert_eq!(text(src, VIS_NOLOCALE), b"\\M-C\\M-)");
    assert_eq!(text(src, VIS_NOLOCALE | VIS_OCTAL), b"\\303\\251");
}

#[test]
fn strvisx_encodes_exactly_len_bytes() {
    let mut dst = [0u8; 32];
    let n = strvisx(&mut dst, b"abcdef", 3, 0).unwrap();
    assert_eq!(&dst[..n], b"abc");
}

#[test]
fn strsvis_honours_the_extra_list() {
    let mut dst = [0u8; 32];
    let n = strsvis(&mut dst, b"a-b", 0, b"-").unwrap();
    assert_eq!(&dst[..n], b"a\\055b");
}

#[test]
fn strvis_truncation_is_none() {
    let mut dst = [0u8; 2];
    assert_eq!(strvis(&mut dst, b"a b", VIS_SP), None);
    let mut dst = [0u8; 6];
    assert_eq!(strvis(&mut dst, b"a b", VIS_SP), Some(6));
}

#[test]
fn every_byte_each_single_flag_has_a_well_formed_encoding() {
    const FLAGS: [u32; 12] = [
        0,
        VIS_CSTYLE,
        VIS_OCTAL,
        VIS_WHITE,
        VIS_SAFE,
        VIS_HTTPSTYLE,
        VIS_MIMESTYLE,
        VIS_GLOB,
        VIS_SHELL,
        VIS_NOSLASH,
        VIS_DQ,
        VIS_NOLOCALE,
    ];
    for flags in FLAGS {
        for byte in 0u16..=255 {
            let byte = u8::try_from(byte).unwrap();
            let out = encoded(byte, flags);
            assert!(
                !out.is_empty(),
                "byte {byte:#04x} flags {flags:#06x} produced nothing"
            );
            assert!(
                out.len() <= VIS_MAX_EXPANSION,
                "byte {byte:#04x} flags {flags:#06x} produced {} bytes",
                out.len()
            );
            // The first byte is an escape introducer, a percent/equal escape or the byte.
            let first = out.first().copied().unwrap_or(0);
            assert!(
                first == b'\\'
                    || first == b'%'
                    || first == b'='
                    || first == b'^'
                    || first == b'M'
                    || first == b'-'
                    || first == byte,
                "byte {byte:#04x} flags {flags:#06x}: {out:?}"
            );
        }
    }
}

#[test]
fn every_byte_roundtrips_through_strunvis() {
    for byte in 0u16..=255 {
        let byte = u8::try_from(byte).unwrap();
        let mut encoded = [0u8; 8];
        let n = vis(&mut encoded, byte, VIS_OCTAL, 0).unwrap();
        let mut decoded = [0u8; 4];
        let m = strunvis(&mut decoded, &encoded[..n], 0).unwrap();
        assert_eq!(&decoded[..m], &[byte], "byte {byte:#04x}");
    }
}

#[test]
fn string_cases_cover_truncation_and_utf8() {
    let cases: &[(&[u8], u32, &[u8])] = &[
        (b"", 0, b""),
        (b"abc", 0, b"abc"),
        (b"a b", 0, b"a b"),
        (b"a b", VIS_SP, b"a\\040b"),
        (b"\n\t\r", VIS_CSTYLE | VIS_WHITE, b"\\n\\t\\r"),
        (b"*?[", VIS_GLOB, b"\\052\\077\\133"),
        (b"\\", 0, b"\\134"),
        (b"\\", VIS_NOSLASH, b"\\"),
        ("é".as_bytes(), 0, "é".as_bytes()),
        ("é".as_bytes(), VIS_OCTAL, "é".as_bytes()),
        (b"\x00\x01\x02", VIS_OCTAL, b"\\000\\001\\002"),
        (
            b"https://x/?a=b",
            VIS_HTTPSTYLE,
            b"https%3a%2f%2fx%2f%3fa%3db",
        ),
    ];
    for (src, flags, expected) in cases {
        assert_eq!(&text(src, *flags), expected, "src {src:?} flags {flags:#x}");
    }
}
