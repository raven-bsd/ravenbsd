// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::strcasestr;

#[test]
fn basics() {
    assert_eq!(strcasestr(b"abcabc", b"BC"), Some(1));
    assert_eq!(strcasestr(b"ABC", b"abc"), Some(0));
    assert_eq!(strcasestr(b"abc", b"z"), None);
    assert_eq!(strcasestr(b"abc", b""), Some(0));
    assert_eq!(strcasestr(b"", b"a"), None);
}

#[test]
fn only_ascii_folds() {
    assert_eq!(strcasestr(&[0x80, 0xc0], &[0xe0]), None);
    assert_eq!(strcasestr(b"xxAyy", b"a"), Some(2));
}

#[test]
fn every_pair_of_short_strings() {
    let alphabet = *b"aAb";
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
    let lower = |byte: u8| byte.to_ascii_lowercase();
    for haystack in &strings {
        for needle in &strings {
            let expected = if needle.is_empty() {
                Some(0)
            } else if needle.len() > haystack.len() {
                None
            } else {
                (0..=haystack.len() - needle.len()).find(|&index| {
                    haystack[index..index + needle.len()]
                        .iter()
                        .zip(needle)
                        .all(|(left, right)| lower(*left) == lower(*right))
                })
            };
            assert_eq!(
                strcasestr(haystack, needle),
                expected,
                "{haystack:?} {needle:?}"
            );
        }
    }
}
