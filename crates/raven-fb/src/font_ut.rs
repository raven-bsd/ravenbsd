// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the 8x16 console font.

use super::*;

#[test]
fn printable_ascii_has_a_full_glyph() {
    for character in FIRST_ASCII..=LAST_ASCII {
        let glyph = glyph(character);
        assert_eq!(glyph.width, 8, "byte 0x{character:02x}");
        assert_eq!(glyph.height, 16, "byte 0x{character:02x}");
        assert_eq!(glyph.data.len(), FONT_HEIGHT, "byte 0x{character:02x}");
    }
}

#[test]
fn a_known_glyph_is_non_blank() {
    let a = glyph(b'A');
    assert!(a.data.iter().any(|row| *row != 0), "the A glyph is blank");
    assert_ne!(a.data, glyph(b' ').data, "A and space are identical");
}

#[test]
fn outside_ascii_uses_the_question_mark_replacement() {
    let replacement = glyph(REPLACEMENT).data;
    for character in [0x00, 0x09, 0x1f, 0x7f, 0x80, 0xff] {
        assert_eq!(glyph(character).data, replacement, "byte 0x{character:02x}");
    }
}
