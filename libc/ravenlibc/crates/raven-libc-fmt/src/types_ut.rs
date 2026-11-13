// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::traits::CharUnit;

#[test]
fn constants_match_the_contract() {
    assert_eq!(DEC_BUF_F64, 800);
    assert_eq!(DEC_BUF_LD, 11_700);
    assert_eq!(SCAN_TOKEN_MAX, 4096);
    assert_eq!(NL_ARGMAX, 64);
    assert_eq!(C_NUMERIC_FORMAT.decimal_point, b".");
    assert_eq!(C_NUMERIC_FORMAT.thousands_sep, b"");
    assert_eq!(C_NUMERIC_FORMAT.grouping, b"");
}

#[test]
fn store_sizes_are_lp64() {
    assert_eq!(StoreSize::Hh.bytes(), 1);
    assert_eq!(StoreSize::H.bytes(), 2);
    assert_eq!(StoreSize::Int.bytes(), 4);
    assert_eq!(StoreSize::L.bytes(), 8);
    assert_eq!(StoreSize::Ll.bytes(), 8);
    assert_eq!(StoreSize::J.bytes(), 8);
    assert_eq!(StoreSize::Z.bytes(), 8);
    assert_eq!(StoreSize::T.bytes(), 8);
}

#[test]
fn char_unit_round_trips() {
    assert_eq!(<u8 as CharUnit>::from_ascii(b'A').to_u32(), 0x41);
    assert_eq!(<u8 as CharUnit>::from_u32(0x141), 0x41);
    assert!(!<u8 as CharUnit>::WIDE);
    assert_eq!(<u32 as CharUnit>::from_ascii(b'A').to_u32(), 0x41);
    assert_eq!(<u32 as CharUnit>::from_u32(0x1f600), 0x1f600);
    assert!(<u32 as CharUnit>::WIDE);
    assert_eq!(<u8 as Default>::default(), 0);
    assert_eq!(<u32 as Default>::default(), 0);
}

#[test]
fn default_options_are_c_locale_and_store() {
    let opts = FmtOptions::default();
    assert_eq!(opts.pct_n, PctN::Store);
    assert_eq!(opts.serrno, 0);
    assert_eq!(opts.numeric.decimal_point, b".");
    assert!(matches!(opts.mode, raven_libc_ctype::MbMode::Byte));
    let scan = ScanOptions::default();
    assert_eq!(scan.numeric.decimal_point, b".");
}
