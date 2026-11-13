// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! Plural expression tests.

use super::*;

#[test]
fn simple_comparisons() {
    assert_eq!(plural_eval(b"n!=1", 1), Some(0));
    assert_eq!(plural_eval(b"n!=1", 2), Some(1));
    assert_eq!(plural_eval(b"n>1", 1), Some(0));
    assert_eq!(plural_eval(b"n>1", 2), Some(1));
    assert_eq!(plural_eval(b"0", 5), Some(0));
    assert_eq!(plural_eval(b"1", 5), Some(1));
}

#[test]
fn ternary_and_boolean_operators() {
    assert_eq!(plural_eval(b"n==1 ? 0 : 1", 1), Some(0));
    assert_eq!(plural_eval(b"n==1 ? 0 : 1", 2), Some(1));
    assert_eq!(plural_eval(b"n>0 && n<10", 5), Some(1));
    assert_eq!(plural_eval(b"n<0 || n>10", 5), Some(0));
}

#[test]
fn russian_expression() {
    let expr = b"n%10==1 && n%100!=11 ? 0 : n%10>=2 && n%10<=4 && (n%100<10 || n%100>=20) ? 1 : 2";
    let expected = [0, 1, 1, 2, 2, 2, 0, 1, 2, 0, 2];
    for (n, value) in [1u64, 2, 4, 5, 11, 12, 21, 22, 25, 101, 111]
        .iter()
        .zip(expected)
    {
        assert_eq!(plural_eval(expr, *n), Some(value), "n={n}");
    }
}

#[test]
fn zero_division_and_depth_are_errors() {
    assert_eq!(plural_eval(b"1/0", 1), None);
    assert_eq!(plural_eval(b"1%0", 1), None);
    let mut expr = Vec::new();
    for _ in 0..(PLURAL_DEPTH + 1) {
        expr.extend_from_slice(b"(");
    }
    expr.push(b'n');
    for _ in 0..(PLURAL_DEPTH + 1) {
        expr.extend_from_slice(b")");
    }
    assert_eq!(plural_eval(&expr, 1), None);
}

#[test]
fn malformed_expressions() {
    for expr in [b"".as_slice(), b"n+", b"n?", b"(n", b"n==1 ? 2", b"@"] {
        assert_eq!(plural_eval(expr, 3), None, "{expr:?}");
    }
}
