// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the built-in ASCII bitmap font.

use super::*;

const A_ROWS: [u8; GLYPH_HEIGHT] = [
    0x00, 0x00, 0x38, 0x38, 0x38, 0x6c, 0x6c, 0x6c, 0x7c, 0xc6, 0xc6, 0xc6, 0x00, 0x00, 0x00, 0x00,
];

#[test]
fn every_printable_ascii_has_an_8x16_glyph() {
    for byte in FIRST_ASCII..=LAST_ASCII {
        let glyph = BasicFont::glyph_for(byte);
        assert_eq!(glyph.width, GLYPH_WIDTH, "byte 0x{byte:02x}");
        assert_eq!(glyph.height, GLYPH_HEIGHT, "byte 0x{byte:02x}");
        assert_eq!(glyph.data.len(), GLYPH_HEIGHT, "byte 0x{byte:02x}");
        assert!(BasicFont.glyph(byte).is_some(), "byte 0x{byte:02x}");
    }
}

#[test]
fn known_glyph_bitmap_is_the_donor_table() {
    let glyph = BasicFont::glyph_for(b'A');
    assert_eq!(glyph.data, A_ROWS.as_slice());
    assert_eq!(
        BasicFont::glyph_for(b' ').data,
        [0u8; GLYPH_HEIGHT].as_slice()
    );
}

#[test]
fn non_printable_and_non_ascii_use_the_question_mark() {
    let replacement = BasicFont::glyph_for(REPLACEMENT).data;
    for byte in [0x00, 0x1f, 0x7f, 0x80, 0xff] {
        assert_eq!(
            BasicFont::glyph_for(byte).data,
            replacement,
            "byte 0x{byte:02x}"
        );
    }
}
