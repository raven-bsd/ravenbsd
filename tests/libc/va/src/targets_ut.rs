// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests of the test targets and the float/scan adapters (`targets.rs`).

use super::*;

#[test]
fn the_real_float_adapter_formats_a_double() {
    let mut out = [0u8; 32];
    let digits = RealFloat
        .dec_digits(
            FloatBits {
                kind: FloatKind::F64,
                bits: 1.5f64.to_bits() as u128,
            },
            2,
            17,
            &mut out,
        )
        .map_err(|_| "digits failed")
        .expect("digits");
    assert_eq!(&out[..digits.len], b"15");
    assert_eq!(digits.decpt, 1);
    assert!(!digits.neg);
    assert!(digits.class == FloatClass::Finite);
}

#[test]
fn the_real_scan_adapter_parses_a_double() {
    let bits = RealScan
        .parse(b"2.5", FloatKind::F64)
        .map_err(|_| "parse failed")
        .expect("parse");
    assert_eq!(bits.bits as u64, 2.5f64.to_bits());
}

#[test]
fn the_slice_source_pushes_back_one_unit() {
    let mut source = SliceSource::new(b"abc");
    assert_eq!(source.getc(), Some(b'a'));
    source.ungetc(b'a');
    assert_eq!(source.getc(), Some(b'a'));
    assert_eq!(source.getc(), Some(b'b'));
    assert_eq!(source.getc(), Some(b'c'));
    assert_eq!(source.getc(), None);
}

#[test]
fn engine_len_formats_through_the_environment() {
    let length = engine_len(b"value=%d", &[42]);
    assert_eq!(length, 8);
}

#[test]
fn an_empty_record_has_no_words() {
    // The shared stores are reset by the tests that own them; this test only checks the
    // value type, so it never races with a recorder target.
    let record = Record::default();
    assert_eq!(record.named, 0);
    assert!(record.words.is_empty());
    assert!(record.values.is_empty());
    assert!(record.doubles.is_empty());
}
