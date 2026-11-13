// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{strcspn, strpbrk, strspn};

#[test]
fn empty_set_rules() {
    assert_eq!(strspn(b"abc", b""), 0);
    assert_eq!(strcspn(b"abc", b""), 3);
    assert_eq!(strpbrk(b"abc", b""), None);
    assert_eq!(strspn(b"", b"a"), 0);
    assert_eq!(strcspn(b"", b"a"), 0);
    assert_eq!(strpbrk(b"", b"a"), None);
}

#[test]
fn spans() {
    assert_eq!(strspn(b"aaabbb", b"a"), 3);
    assert_eq!(strcspn(b"aaabbb", b"b"), 3);
    assert_eq!(strpbrk(b"aaabbb", b"b"), Some(3));
    assert_eq!(strspn(b"abc", b"abc"), 3);
    assert_eq!(strcspn(b"abc", b"z"), 3);
    assert_eq!(strpbrk(b"abc", b"z"), None);
}

#[test]
fn embedded_nul_ends_the_string() {
    assert_eq!(strspn(b"aa\0bb", b"a"), 2);
    assert_eq!(strcspn(b"aa\0bb", b"b"), 2);
    assert_eq!(strpbrk(b"aa\0bb", b"b"), None);
}

#[test]
fn high_bytes() {
    assert_eq!(strspn(&[0x80, 0x80, 0xff], &[0x80]), 2);
    assert_eq!(strcspn(&[0x80, 0x80, 0xff], &[0xff]), 2);
    assert_eq!(strpbrk(&[0x80, 0x80, 0xff], &[0xff]), Some(2));
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
    for haystack in &strings {
        for set in &strings {
            let span = haystack
                .iter()
                .take_while(|byte| set.contains(byte))
                .count();
            let cspan = haystack
                .iter()
                .take_while(|byte| !set.contains(byte))
                .count();
            let pbrk = haystack.iter().position(|byte| set.contains(byte));
            assert_eq!(strspn(haystack, set), span, "{haystack:?} {set:?}");
            assert_eq!(strcspn(haystack, set), cspan, "{haystack:?} {set:?}");
            assert_eq!(strpbrk(haystack, set), pbrk, "{haystack:?} {set:?}");
        }
    }
}
