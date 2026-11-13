// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exhaustive Unicode data check: every scalar against an independent per-code-point oracle.

#[path = "support/ucd_oracle.rs"]
mod ucd_oracle;

use std::path::{Path, PathBuf};

use raven_libc_ctype::*;

fn ucd() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../xtask/data/ucd-15.1.0")
}

#[test]
fn every_code_point_agrees_with_the_oracle() {
    let oracle = ucd_oracle::load(&ucd()).expect("load UCD oracle");
    for cp in 0..=0x10ffffu32 {
        assert_eq!(iswalpha(cp), oracle.is_alpha(cp), "iswalpha(U+{cp:04X})");
        assert_eq!(iswpunct(cp), oracle.is_punct(cp), "iswpunct(U+{cp:04X})");
        assert_eq!(towupper(cp), oracle.upper(cp), "towupper(U+{cp:04X})");
        assert_eq!(towlower(cp), oracle.lower(cp), "towlower(U+{cp:04X})");
        assert_eq!(iswlower(cp), oracle.upper(cp) != cp, "iswlower(U+{cp:04X})");
        assert_eq!(iswupper(cp), oracle.lower(cp) != cp, "iswupper(U+{cp:04X})");
        assert_eq!(wcwidth(cp), oracle.width(cp), "wcwidth(U+{cp:04X})");
    }
}

#[test]
fn reference_values_of_the_table_design() {
    assert!(iswalpha(0x0660));
    assert!(iswpunct(0x0300));
    assert!(iswpunct(0x00a0));
    assert!(iswpunct(0x20ac));
    assert!(iswpunct(0x1f600));
    assert!(!iswpunct(0x3000));
    assert!(!iswpunct(0x4e00));
    assert_eq!(wcwidth(0x4e00), 2);
    assert_eq!(wcwidth(0x0300), 0);
    assert_eq!(wcwidth(0x00ad), 1);
    assert_eq!(wcwidth(0x1f600), 2);
    assert_eq!(wcwidth(0x200b), 0);
    assert_eq!(wcwidth(0x7f), -1);
    assert_eq!(wcwidth(0), 0);
    assert_eq!(towupper(0x01c5), 0x01c4);
    assert_eq!(towlower(0x01c5), 0x01c6);
    assert_eq!(towupper(0x0131), 0x0049);
    assert_eq!(towlower(0x0130), 0x0069);
    assert_eq!(towupper(0x00df), 0x00df);
}

#[test]
fn titlecase_digraphs_map_both_ways() {
    for (title, upper, lower) in [
        (0x01c5, 0x01c4, 0x01c6),
        (0x01c8, 0x01c7, 0x01c9),
        (0x01cb, 0x01ca, 0x01cc),
        (0x01f2, 0x01f1, 0x01f3),
    ] {
        assert_eq!(towupper(title), upper);
        assert_eq!(towlower(title), lower);
    }
}

#[test]
fn emoji_presentation_has_width_two_except_the_regional_indicators() {
    let oracle = ucd_oracle::load(&ucd()).expect("load UCD oracle");
    for cp in 0..=0x10ffffu32 {
        if !oracle.emoji_presentation[cp as usize] {
            continue;
        }
        if (0x1f1e6..=0x1f1ff).contains(&cp) {
            // East_Asian_Width is N for the 26 regional indicators (Unicode 15.1), so the
            // fixed WIDE = W/F rule gives them width 1. RD-new D-ct-7.
            assert_eq!(wcwidth(cp), 1);
            continue;
        }
        assert_eq!(wcwidth(cp), 2, "Emoji_Presentation U+{cp:04X}");
    }
}

#[test]
fn white_space_minus_the_five_excluded_code_points_is_iswspace() {
    let oracle = ucd_oracle::load(&ucd()).expect("load UCD oracle");
    for cp in 0..=0x10ffffu32 {
        let excluded = matches!(cp, 0x00a0 | 0x1680 | 0x180e | 0x2007 | 0x202f);
        let expected = oracle.white_space[cp as usize] && !excluded;
        assert_eq!(iswspace(cp), expected, "iswspace(U+{cp:04X})");
    }
}
