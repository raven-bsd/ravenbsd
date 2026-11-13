// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::test_support::RecordingSource;

fn lex(input: &[u8], limit: usize) -> (Vec<u8>, Vec<u8>) {
    let mut src = RecordingSource::new(input.to_vec());
    let mut f = Limited::new(&mut src);
    f.shlim(limit);
    let mut buf = [0u8; 64];
    let n = parse_token(&mut f, &mut buf);
    let token = buf[..n].to_vec();
    f.finish();
    (token, src.ungets.iter().rev().copied().collect())
}

#[test]
fn decimal_forms() {
    assert_eq!(lex(b"1.5", 0).0, b"1.5");
    assert_eq!(lex(b"-1.5", 0).0, b"-1.5");
    assert_eq!(lex(b"+1.5", 0).0, b"+1.5");
    assert_eq!(lex(b"1e3", 0).0, b"1e3");
    assert_eq!(lex(b"1E-3", 0).0, b"1E-3");
    assert_eq!(lex(b".5", 0).0, b".5");
    assert_eq!(lex(b"5.", 0).0, b"5.");
}

#[test]
fn exponent_without_digits_is_not_part_of_the_token() {
    assert_eq!(lex(b"1.5e", 0).0, b"1.5");
    assert_eq!(lex(b"1.5e+", 0).0, b"1.5");
    assert_eq!(lex(b"1e", 0).0, b"1");
}

#[test]
fn hex_forms() {
    assert_eq!(lex(b"0x1.8p+0", 0).0, b"0x1.8p+0");
    assert_eq!(lex(b"0X1P-3", 0).0, b"0X1P-3");
    assert_eq!(lex(b"0x", 0).0, b"0");
    assert_eq!(lex(b"0x.", 0).0, b"0");
}

#[test]
fn infinity_and_nan() {
    assert_eq!(lex(b"inf", 0).0, b"inf");
    assert_eq!(lex(b"INF", 0).0, b"INF");
    assert_eq!(lex(b"infinity", 0).0, b"infinity");
    assert_eq!(lex(b"infin", 0).0, b"inf");
    assert_eq!(lex(b"nan", 0).0, b"nan");
    assert_eq!(lex(b"NAN", 0).0, b"NAN");
    assert_eq!(lex(b"nan(0x1)", 0).0, b"nan(0x1)");
    assert_eq!(lex(b"nan(", 0).0, b"nan");
}

#[test]
fn the_width_limit_splits_the_token() {
    assert_eq!(lex(b"1.5", 2).0, b"1.");
    assert_eq!(lex(b"12345", 3).0, b"123");
}

#[test]
fn the_rejected_character_is_pushed_back() {
    let (token, ungot) = lex(b"1.5x", 0);
    assert_eq!(token, b"1.5");
    assert_eq!(ungot, b"x");
    let (token, ungot) = lex(b"infx", 0);
    assert_eq!(token, b"inf");
    assert_eq!(ungot, b"x");
}

#[test]
fn an_empty_token_is_zero_length() {
    assert_eq!(lex(b"", 0).0, b"");
    assert_eq!(lex(b"x", 0).0, b"");
    assert_eq!(lex(b" + ", 0).0, b"");
}
