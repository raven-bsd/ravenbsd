// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{strcmp, strncmp};

fn sign(value: i32) -> i32 {
    value.signum()
}

#[test]
fn equal_strings() {
    assert_eq!(strcmp(b"", b""), 0);
    assert_eq!(strcmp(b"abc", b"abc"), 0);
    assert_eq!(strcmp(b"\0abc", b""), 0);
}

#[test]
fn unsigned_compare() {
    let cases: &[(&[u8], &[u8], i32)] = &[
        (&[0x7f], &[0x80], -1),
        (&[0x80], &[0x7f], 1),
        (&[0xff], &[0x00], 1),
        (&[0x80], &[0xff], -1),
        (&[0x00], &[0xff], -1),
    ];
    for (left, right, expected) in cases {
        assert_eq!(sign(strcmp(left, right)), *expected);
    }
}

#[test]
fn high_bytes_at_every_position() {
    let bytes = [0x7f, 0x80, 0xff, 0x00];
    for position in 0usize..4 {
        let mut left = *b"xxxx";
        let mut right = *b"xxxx";
        left[position] = bytes[position];
        right[position] = bytes[position ^ 1];
        assert_eq!(sign(strcmp(&left, &right)), -sign(strcmp(&right, &left)));
        assert_eq!(strcmp(&left, &right), -strcmp(&right, &left));
    }
}

#[test]
fn prefix_is_smaller() {
    assert_eq!(sign(strcmp(b"abc", b"abcd")), -1);
    assert_eq!(sign(strcmp(b"abcd", b"abc")), 1);
}

#[test]
fn strncmp_zero_is_equal() {
    assert_eq!(strncmp(b"abc", b"xyz", 0), 0);
    assert_eq!(strncmp(b"abc", b"abd", 3), strcmp(b"abc", b"abd"));
    assert_eq!(strncmp(b"abc", b"abd", 2), 0);
    assert_eq!(sign(strncmp(b"abc", b"ab", 5)), 1);
}

#[test]
fn strncmp_stops_at_nul() {
    assert_eq!(strncmp(b"a\0b", b"a\0c", 3), 0);
    assert_eq!(strncmp(b"a", b"a\0b", 3), 0);
}

#[test]
fn every_pair_of_short_strings() {
    let alphabet = [b'a', b'b', 0x80];
    let mut strings = vec![Vec::new()];
    for _ in 0..3 {
        let mut next = Vec::new();
        for prefix in &strings {
            for &byte in &alphabet {
                let mut value = prefix.clone();
                value.push(byte);
                next.push(value);
            }
        }
        strings.extend(next.iter().cloned());
    }
    for left in &strings {
        for right in &strings {
            let expected = {
                let common = left.len().min(right.len());
                let mut result = 0;
                for index in 0..common {
                    if left[index] != right[index] {
                        result = i32::from(left[index]) - i32::from(right[index]);
                        break;
                    }
                }
                if result == 0 && left.len() != right.len() {
                    // The slice end acts as NUL; the longer string has a non-NUL byte there.
                    let (longer, shorter) = if left.len() > right.len() {
                        (left, right)
                    } else {
                        (right, left)
                    };
                    let byte = longer[shorter.len()];
                    let value = i32::from(byte);
                    if left.len() > right.len() {
                        value
                    } else {
                        -value
                    }
                } else {
                    result
                }
            };
            assert_eq!(
                sign(strcmp(left, right)),
                sign(expected),
                "{left:?} {right:?}"
            );
        }
    }
}
