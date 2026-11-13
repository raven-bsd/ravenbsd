// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the `unvis` decoder: one case per donor escape form, the bad forms and the
//! `UNVIS_END` flush. The expected bytes are those of `netbsd:lib/libc/gen/unvis.c`.

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

use super::*;

fn decode(src: &[u8], flags: u32) -> Option<Vec<u8>> {
    let mut dst = [0u8; 256];
    let n = strunvis(&mut dst, src, flags)?;
    Some(dst[..n].to_vec())
}

#[test]
fn plain_text_passes_through() {
    assert_eq!(decode(b"hello", 0).unwrap(), b"hello");
    assert_eq!(decode(b"", 0).unwrap(), b"");
}

#[test]
fn c_style_escapes() {
    assert_eq!(decode(b"\\n", 0).unwrap(), b"\n");
    assert_eq!(decode(b"\\t", 0).unwrap(), b"\t");
    assert_eq!(decode(b"\\r", 0).unwrap(), b"\r");
    assert_eq!(decode(b"\\b", 0).unwrap(), b"\x08");
    assert_eq!(decode(b"\\a", 0).unwrap(), b"\x07");
    assert_eq!(decode(b"\\v", 0).unwrap(), b"\x0b");
    assert_eq!(decode(b"\\f", 0).unwrap(), b"\x0c");
    assert_eq!(decode(b"\\s", 0).unwrap(), b" ");
    assert_eq!(decode(b"\\E", 0).unwrap(), b"\x1b");
    assert_eq!(decode(b"\\\\", 0).unwrap(), b"\\");
    assert_eq!(decode(b"\\$", 0).unwrap(), b"");
}

#[test]
fn meta_and_control_forms() {
    assert_eq!(decode(b"\\M-a", 0).unwrap(), b"\xe1");
    assert_eq!(decode(b"\\M-\x01", 0).unwrap(), b"\x81");
    assert_eq!(decode(b"\\^A", 0).unwrap(), b"\x01");
    assert_eq!(decode(b"\\^?", 0).unwrap(), b"\x7f");
    assert_eq!(decode(b"\\M-?", 0).unwrap(), b"\xbf");
}

#[test]
fn octal_forms() {
    assert_eq!(decode(b"\\101", 0).unwrap(), b"A");
    assert_eq!(decode(b"\\0", 0).unwrap(), b"\0");
    assert_eq!(decode(b"\\12", 0).unwrap(), b"\n");
    assert_eq!(decode(b"\\377", 0).unwrap(), b"\xff");
    // The third digit overflow check: 040 << 3 would exceed 8 bits.
    assert_eq!(decode(b"\\477", 0), None);
}

#[test]
fn octal_pushback_and_flush() {
    // "\\101x": after the third digit the 'x' is pushed back and decoded normally.
    assert_eq!(decode(b"\\101x", 0).unwrap(), b"Ax");
    // "\\12" ends in S_OCTAL2; UNVIS_END flushes it.
    assert_eq!(decode(b"\\12", 0).unwrap(), b"\n");
    // "\\1" ends in S_OCTAL2 too.
    assert_eq!(decode(b"\\1", 0).unwrap(), b"\x01");
}

#[test]
fn hex_forms() {
    assert_eq!(decode(b"\\x41", 0).unwrap(), b"A");
    assert_eq!(decode(b"\\x4", 0).unwrap(), b"\x04");
    assert_eq!(decode(b"\\xg", 0), None);
    assert_eq!(decode(b"%41", VIS_HTTP1808).unwrap(), b"A");
    assert_eq!(decode(b"%4", VIS_HTTP1808).unwrap(), b"\x04");
    // The HTTP pushback path yields the partial byte and re-feeds the non-hex character.
    assert_eq!(decode(b"%zz", VIS_HTTP1808).unwrap(), b"\0zz");
}

#[test]
fn mime_forms() {
    assert_eq!(decode(b"=41", VIS_MIMESTYLE).unwrap(), b"A");
    assert_eq!(decode(b"=3D", VIS_MIMESTYLE).unwrap(), b"=");
    // Lower-case hex is rejected by the MIME path (the donor requires digits or upper case).
    assert_eq!(decode(b"=4a", VIS_MIMESTYLE), None);
    // Soft line break: "=CRLF" continues.
    assert_eq!(decode(b"a=\r\nb", VIS_MIMESTYLE).unwrap(), b"ab");
    assert_eq!(decode(b"a=\nb", VIS_MIMESTYLE).unwrap(), b"ab");
}

#[test]
fn rfc1866_named_and_numeric() {
    assert_eq!(decode(b"&amp;", VIS_HTTP1866).unwrap(), b"&");
    assert_eq!(decode(b"&lt;", VIS_HTTP1866).unwrap(), b"<");
    assert_eq!(decode(b"&nbsp;", VIS_HTTP1866).unwrap(), b"\xa0");
    assert_eq!(decode(b"&#65;", VIS_HTTP1866).unwrap(), b"A");
    assert_eq!(decode(b"&#0;", VIS_HTTP1866).unwrap(), b"\0");
    assert_eq!(decode(b"&#255;", VIS_HTTP1866).unwrap(), b"\xff");
    assert_eq!(decode(b"&#256;", VIS_HTTP1866), None);
    assert_eq!(decode(b"&nosuch;", VIS_HTTP1866), None);
    assert_eq!(decode(b"&amq;", VIS_HTTP1866), None);
}

#[test]
fn html_1866_without_flag_is_literal() {
    assert_eq!(decode(b"&amp;", 0).unwrap(), b"&amp;");
}

#[test]
fn noescape_makes_backslash_literal() {
    assert_eq!(decode(b"\\n", VIS_NOESCAPE).unwrap(), b"\\n");
}

#[test]
fn bad_sequences_reset_the_state() {
    // A backslash before a graph character is the donor's valid "quoted" form.
    assert_eq!(decode(b"\\q", 0).unwrap(), b"q");
    assert_eq!(decode(b"\\Mq", 0), None);
    assert_eq!(decode(b"\\xg", 0), None);
    assert_eq!(decode(b"\\xg", 0), None);
}

#[test]
fn end_state_flush() {
    let mut machine = Unvis::new();
    assert_eq!(machine.feed(b'\\', 0), UnvisStep::NoChar);
    assert_eq!(machine.feed(b'0', 0), UnvisStep::NoChar);
    assert_eq!(machine.feed(b'1', 0), UnvisStep::NoChar);
    // UNVIS_END in the flag is the donor's flush entry.
    assert_eq!(machine.feed(0, UNVIS_END), UnvisStep::Valid(0o1));
    let mut ground = Unvis::new();
    assert_eq!(ground.feed(b'a', 0), UnvisStep::Valid(b'a'));
    assert_eq!(ground.finish(), UnvisStep::NoChar);
}

#[test]
fn truncated_output_is_none() {
    let mut dst = [0u8; 1];
    assert_eq!(strunvis(&mut dst, b"ab", 0), None);
    assert_eq!(strnunvis(&mut dst, b"ab", 0), None);
    assert_eq!(strunvisx(&mut dst, b"a", 0), Some(1));
    assert_eq!(strnunvisx(&mut dst, b"a", 0), Some(1));
}

#[test]
fn vis_roundtrip_for_every_byte_in_octal() {
    for byte in 0u16..=255 {
        let byte = u8::try_from(byte).unwrap();
        let encoded = alloc::format!("\\{:03o}", byte);
        let decoded = decode(encoded.as_bytes(), 0).unwrap();
        assert_eq!(decoded, alloc::vec![byte], "byte {byte:#04x}");
    }
}
