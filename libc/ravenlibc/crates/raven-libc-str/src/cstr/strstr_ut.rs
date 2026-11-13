// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::strstr;

fn naive(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .find(|&index| &haystack[index..index + needle.len()] == needle)
}

#[test]
fn basics() {
    assert_eq!(strstr(b"abcabc", b"bc"), Some(1));
    assert_eq!(strstr(b"abcabc", b"abc"), Some(0));
    assert_eq!(strstr(b"abcabc", b"z"), None);
    assert_eq!(strstr(b"abc", b""), Some(0));
    assert_eq!(strstr(b"", b""), Some(0));
    assert_eq!(strstr(b"", b"a"), None);
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
            assert_eq!(
                strstr(haystack, needle),
                naive(haystack, needle),
                "{haystack:?} {needle:?}"
            );
        }
    }
}

#[test]
fn overlapping_candidates() {
    assert_eq!(strstr(b"aaaaa", b"aaa"), Some(0));
    assert_eq!(strstr(b"baaaaa", b"aaa"), Some(1));
    assert_eq!(strstr(b"abababa", b"aba"), Some(0));
    assert_eq!(strstr(b"xxabababa", b"ababa"), Some(2));
}

#[test]
fn long_needle() {
    let needle = vec![b'x'; 200];
    let mut haystack = vec![b'.'; 5000];
    for (offset, &byte) in needle.iter().enumerate() {
        haystack[1234 + offset] = byte;
    }
    assert_eq!(strstr(&haystack, &needle), Some(1234));
}
