// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::strnstr;

#[test]
fn basics() {
    assert_eq!(strnstr(b"abcabc", b"bc", 6), Some(1));
    assert_eq!(strnstr(b"abcabc", b"bc", 2), None);
    assert_eq!(strnstr(b"abcabc", b"bc", 3), Some(1));
    assert_eq!(strnstr(b"abc", b"", 3), Some(0));
    assert_eq!(strnstr(b"", b"a", 0), None);
}

#[test]
fn stops_at_nul() {
    assert_eq!(strnstr(b"ab\0cd", b"cd", 5), None);
    assert_eq!(strnstr(b"ab\0cd", b"b", 5), Some(1));
    assert_eq!(strnstr(b"ab\0cd", b"\0", 5), None);
}

#[test]
fn needle_longer_than_limit() {
    assert_eq!(strnstr(b"abcdef", b"def", 3), None);
    assert_eq!(strnstr(b"abcdef", b"def", 6), Some(3));
    assert_eq!(strnstr(b"abcdef", b"def", 5), None);
}

#[test]
fn every_pair_of_short_strings() {
    let alphabet = *b"ab";
    let mut strings = vec![Vec::new()];
    for _ in 0..4 {
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
    for haystack in &strings {
        for needle in &strings {
            for len in 0usize..=haystack.len() {
                let expected = if needle.is_empty() {
                    Some(0)
                } else if needle.len() > len {
                    None
                } else {
                    (0..=len - needle.len())
                        .find(|&index| &haystack[index..index + needle.len()] == needle)
                };
                assert_eq!(
                    strnstr(haystack, needle, len),
                    expected,
                    "{haystack:?} {needle:?} {len}"
                );
            }
        }
    }
}
