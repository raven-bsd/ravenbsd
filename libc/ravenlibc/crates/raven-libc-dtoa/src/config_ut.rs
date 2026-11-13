// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::*;
use crate::types::{FPI_ROUND_UP, Fpi};

#[test]
fn accessors_word0_word1() {
    assert_eq!(word0(1.0), 0x3ff0_0000);
    assert_eq!(word1(1.0), 0);
    assert_eq!(word0(-0.0), 0x8000_0000);
    assert_eq!(word1(-0.0), 0);
    assert_eq!(word0(-1.0), 0xbff0_0000);
    // Smallest subnormal: 0x0000_0000_0000_0001.
    let min = f64::from_bits(1);
    assert_eq!(word0(min), 0);
    assert_eq!(word1(min), 1);
    // A quiet NaN payload survives the round trip through the accessors.
    let mut nan = from_words(0x7ff8_0000, 0x1234_5678);
    assert!(nan.is_nan());
    assert_eq!(word0(nan), 0x7ff8_0000);
    assert_eq!(word1(nan), 0x1234_5678);
    set_word0(&mut nan, 0x7ff8_0001);
    set_word1(&mut nan, 0x0000_0002);
    assert_eq!(nan.to_bits(), 0x7ff8_0001_0000_0002);
    nan = f64::from_bits(0x7ff8_0000_0000_0000);
    set_word0(&mut nan, 0x7ff8_0000);
    assert_eq!(word1(nan), 0);
}

#[test]
fn nan_patterns_are_canonical() {
    let f = f32::from_bits(F_QNAN);
    assert!(f.is_nan());
    assert_eq!(f.to_bits(), 0x7fc0_0000);
    let d = from_words(D_QNAN1, D_QNAN0);
    assert!(d.is_nan());
    assert_eq!(d.to_bits(), 0x7ff8_0000_0000_0000);
    // x87 slots are checked where the shim builds them (fmt_ld80_ut).
    assert_eq!(LDUS_QNAN[4] & 0x7fff, 0x7fff);
    assert_eq!(LDUS_QNAN[3] & 0xc000, 0xc000);
    assert_eq!(LD_QNAN[3], 0x7fff_8000);
}

#[test]
fn tables_match_the_donor_values() {
    assert_eq!(TENS.len(), 23);
    assert_eq!(TENS[0], 1.0);
    assert_eq!(TENS[22], 1e22);
    assert_eq!(BIGTENS.len(), 5);
    assert_eq!(BIGTENS[0], 1e16);
    assert_eq!(BIGTENS[4], 1e256);
    assert_eq!(TINYTENS[0], 1e-16);
    assert_eq!(TINYTENS[4], 1e-256);
    // The scaled entry is 1e-256 * 2^106, exactly (the donor writes it as a product).
    assert_eq!(
        STRTO_TINYTENS[4],
        1e-256 * 9007199254740992.0 * 9007199254740992.0
    );
    assert_eq!(STRTO_TINYTENS[0], TINYTENS[0]);
}

#[test]
fn at_treats_the_end_as_nul() {
    let bytes = b"1e+";
    assert_eq!(at(bytes, 0), b'1');
    assert_eq!(at(bytes, 2), b'+');
    assert_eq!(at(bytes, 3), 0);
    assert_eq!(at(bytes, 1000), 0);
}

#[test]
fn float_double_and_ld_formats_match_the_donor() {
    let f = Fpi::float();
    assert_eq!(f.nbits, 24);
    assert_eq!(f.emin, 1 - 127 - 24 + 1);
    assert_eq!(f.emax, 254 - 127 - 24 + 1);
    let d = Fpi::double();
    assert_eq!(d.nbits, 53);
    assert_eq!(d.emin, -1074);
    assert_eq!(d.emax, 971);
    let x = Fpi::ld80();
    assert_eq!(x.nbits, 64);
    assert_eq!(x.emin, -16445);
    assert_eq!(x.emax, 16320);
    let q = Fpi::ld128();
    assert_eq!(q.nbits, 113);
    assert_eq!(q.emin, -16494);
    assert_eq!(q.emax, 16271);
    assert_eq!(q.with_rounding(FPI_ROUND_UP).rounding, 2);
}
