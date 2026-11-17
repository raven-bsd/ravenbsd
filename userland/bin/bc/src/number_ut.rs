// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the bounded decimal number core.

use super::*;

fn number(text: &str) -> Number {
    Number::parse_decimal(text).unwrap()
}

fn text(value: &Number) -> String {
    value.to_decimal_string()
}

#[test]
fn parses_and_prints_decimals() {
    assert_eq!(text(&number("1.20")), "1.20");
    assert_eq!(text(&number(".50")), ".50");
    assert_eq!(text(&number("-.5")), "-.5");
    assert_eq!(text(&number("0.00")), "0");
    assert_eq!(text(&number("100")), "100");
    assert!(Number::parse_decimal(".").is_none());
    assert!(Number::parse_decimal("x").is_none());
}

#[test]
fn adds_and_subtracts_with_max_scale() {
    assert_eq!(text(&number("1.20").add(&number("2.0"))), "3.20");
    assert_eq!(text(&number("1.20").sub(&number("1.20"))), "0");
    assert_eq!(number("1.20").sub(&number("1.20")).scale(), 2);
    assert_eq!(text(&number("-1.2").add(&number("2.0"))), ".8");
    assert_eq!(text(&number("-1.2").sub(&number("2.0"))), "-3.2");
}

#[test]
fn multiplication_uses_the_bc_scale_rule() {
    assert_eq!(text(&number("1.20").mul(&number("2.0"), 2)), "2.40");
    assert_eq!(text(&number("1.2").mul(&number("1.2"), 0)), "1.4");
    assert_eq!(text(&number("1.2").mul(&number("1.2"), 1)), "1.4");
    assert_eq!(text(&number("1.2").mul(&number("1.2"), 3)), "1.44");
    assert_eq!(text(&number("1.20").mul(&number("0"), 2)), "0");
    assert_eq!(number("1.20").mul(&number("0"), 2).scale(), 2);
}

#[test]
fn division_truncates_to_global_scale() {
    assert_eq!(text(&number("5").div(&number("2"), 2).unwrap()), "2.50");
    assert_eq!(text(&number("5").div(&number("2"), 0).unwrap()), "2");
    assert_eq!(text(&number("-5").div(&number("2"), 2).unwrap()), "-2.50");
    assert!(number("1").div(&number("0"), 2).is_none());
}

#[test]
fn remainder_uses_bc_division() {
    assert_eq!(text(&number("5").rem(&number("2"), 0).unwrap()), "1");
    assert_eq!(text(&number("5").rem(&number("2"), 2).unwrap()), "0");
    assert_eq!(text(&number("5.5").rem(&number("2"), 2).unwrap()), "0");
    assert_eq!(text(&number("5.5").rem(&number("2.5"), 2).unwrap()), "0");
    assert!(number("5").rem(&number("0"), 2).is_none());
}

#[test]
fn power_and_sqrt() {
    assert_eq!(text(&number("2").pow(&number("10"), 0).unwrap()), "1024");
    assert_eq!(text(&number("2").pow(&number("-2"), 3).unwrap()), ".250");
    assert_eq!(text(&number("1.2").pow(&number("2"), 0).unwrap()), "1.4");
    assert_eq!(text(&number("1.2").pow(&number("2"), 3).unwrap()), "1.44");
    assert_eq!(text(&number("2").sqrt(2).unwrap()), "1.41");
    assert_eq!(text(&number("4.00").sqrt(0).unwrap()), "2.00");
    assert_eq!(text(&number(".25").sqrt(0).unwrap()), ".50");
    assert!(number("-1").sqrt(2).is_none());
    assert!(number("2").pow(&number("1.5"), 2).is_none());
}

#[test]
fn length_and_scale() {
    assert_eq!(number("123.45").length(), 5);
    assert_eq!(number("0").length(), 1);
    assert_eq!(number(".001").length(), 3);
    assert_eq!(number(".0100").length(), 4);
    assert_eq!(number("100.00").length(), 5);
    assert_eq!(number("1.230").scale(), 3);
}

#[test]
fn compares_and_truncates() {
    assert_eq!(number("1.20").compare(&number("1.2")), Ordering::Equal);
    assert_eq!(number("-2").compare(&number("1")), Ordering::Less);
    assert_eq!(number("2.5").to_i64(), Some(2));
    assert_eq!(number("-2.5").to_i64(), Some(-2));
    assert!(!number("1.5").is_integer());
    assert!(number("2.00").is_integer());
}

#[test]
fn with_scale_pads_and_truncates() {
    assert_eq!(text(&number("1.2").with_scale(3)), "1.200");
    assert_eq!(text(&number("1.234").with_scale(2)), "1.23");
    assert_eq!(text(&number("-1.234").with_scale(2)), "-1.23");
}
