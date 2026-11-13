// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests of the strip and character-set representation.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

use super::*;
use crate::locale::{ByteLocale, Utf8Locale};

#[test]
fn sop_assembles_and_splits() {
    let word = sop(OCHAR, 0x41);
    assert_eq!(op(word), OCHAR);
    assert_eq!(opnd(word), 0x41);
    let other = sop(OANYOF, 12);
    assert_eq!(op(other), OANYOF);
    assert_eq!(opnd(other), 12);
}

#[test]
fn opcodes_are_in_donor_order() {
    let ordered = [
        OEND, OCHAR, OBOL, OEOL, OANY, OANYOF, OBACK_, O_BACK, OPLUS_, O_PLUS, OQUEST_, O_QUEST,
        OLPAREN, ORPAREN, OCH_, OOR1, OOR2, O_CH, OBOW, OEOW, OBOS, OEOS, OWBND, ONWBND,
    ];
    for (index, value) in ordered.iter().enumerate() {
        assert_eq!(*value, ((index as u32) + 1) << OPSHIFT);
    }
}

#[test]
fn operand_is_masked_to_twenty_seven_bits() {
    let word = sop(OCHAR, 0x0800_0041);
    assert_eq!(opnd(word), 0x41);
}

#[test]
fn cset_bitmap_membership_and_inversion() {
    let mut cs = Cset::default();
    cs.bmp[97 >> 3] = 1 << (97 & 7);
    assert!(cs.chin(i32::from(b'a'), &ByteLocale, 256));
    assert!(!cs.chin(i32::from(b'b'), &ByteLocale, 256));
    cs.invert = true;
    assert!(!cs.chin(i32::from(b'a'), &ByteLocale, 256));
    assert!(cs.chin(i32::from(b'b'), &ByteLocale, 256));
}

#[test]
fn cset_wide_ranges_and_types() {
    let mut cs = Cset::default();
    cs.wides.push(0x20ac);
    cs.ranges.push(CsetRange {
        min: 0x400,
        max: 0x4ff,
    });
    assert!(cs.chin(0x20ac, &ByteLocale, 128));
    assert!(cs.chin(0x450, &ByteLocale, 128));
    assert!(!cs.chin(0x500, &ByteLocale, 128));

    let mut typed = Cset::default();
    typed.types.push(Wctype::Alpha);
    assert!(typed.chin(0x4e2d, &Utf8Locale, 128));
    assert!(!typed.chin(0x20ac, &Utf8Locale, 128));
}

#[test]
fn cset_icase_pairs_with_the_wides() {
    let mut cs = Cset {
        icase: true,
        ..Cset::default()
    };
    cs.wides.push(0xc9);
    assert!(cs.chin(0xe9, &ByteLocale, 128));
    assert!(cs.chin(0xc9, &ByteLocale, 128));
}

#[test]
fn isword_truncates_to_a_byte() {
    // The donor quirk: (uch)(c) before iswalnum, so U+00E9 -> 0xE9 (alpha) but U+4E2D -> '-'.
    assert!(isword(0xe9, &ByteLocale));
    assert!(!isword(0x4e2d, &ByteLocale));
    assert!(isword(i32::from(b'_'), &ByteLocale));
    assert!(!isword(i32::from(b'-'), &ByteLocale));
}

#[test]
fn magic_checks_the_inner_word() {
    let mut guts = ReGuts::default();
    assert!(!guts.magic_ok());
    guts.magic = crate::consts::MAGIC2;
    assert!(guts.magic_ok());
    assert_eq!(guts.word(1000), OEND);
}
