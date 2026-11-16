// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the bounded `-l` math library.

use crate::number::Number;

use super::{arctangent, cosine, exponential, logarithm, sine};

fn number(text: &str) -> Number {
    Number::parse_decimal(text).unwrap()
}

#[test]
fn exponential_and_logarithm() {
    assert_eq!(
        exponential(&number("1"), 10).unwrap().to_decimal_string(),
        "2.7182818284"
    );
    assert_eq!(
        logarithm(&number("2"), 10).unwrap().to_decimal_string(),
        ".6931471805"
    );
    assert_eq!(
        exponential(&number("1"), 20).unwrap().to_decimal_string(),
        "2.71828182845904523536"
    );
    assert_eq!(
        logarithm(&number("2"), 20).unwrap().to_decimal_string(),
        ".69314718055994530941"
    );
    assert!(logarithm(&number("0"), 10).is_none());
    assert!(logarithm(&number("-1"), 10).is_none());
}

#[test]
fn trigonometric_functions() {
    assert_eq!(
        sine(&number("1"), 10).unwrap().to_decimal_string(),
        ".8414709848"
    );
    assert_eq!(
        cosine(&number("1"), 10).unwrap().to_decimal_string(),
        ".5403023058"
    );
    assert_eq!(
        arctangent(&number("1"), 10).unwrap().to_decimal_string(),
        ".7853981633"
    );
    assert_eq!(
        arctangent(&number("-1"), 10).unwrap().to_decimal_string(),
        "-.7853981633"
    );
}
