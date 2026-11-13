// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::find_sub;

fn naive(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&index| {
        haystack
            .get(index..index + needle.len())
            .is_some_and(|window| window == needle)
    })
}

/// Every string over `{a,b,0}` of length 0..=4.
fn strings(alphabet: &[u8], max_len: usize) -> Vec<Vec<u8>> {
    let mut all = vec![Vec::new()];
    let mut level = vec![Vec::new()];
    for _ in 0..max_len {
        let mut next = Vec::new();
        for prefix in &level {
            for &byte in alphabet {
                let mut value = prefix.clone();
                value.push(byte);
                next.push(value);
            }
        }
        all.extend(next.iter().cloned());
        level = next;
    }
    all
}

#[test]
fn exhaustive_small_domain() {
    let domain = strings(&[b'a', b'b', 0], 4);
    for haystack in &domain {
        for needle in &domain {
            assert_eq!(
                find_sub(haystack, needle),
                naive(haystack, needle),
                "haystack {haystack:?} needle {needle:?}"
            );
        }
    }
}

#[test]
fn empty_needle_matches_at_zero() {
    assert_eq!(find_sub(b"", b""), Some(0));
    assert_eq!(find_sub(b"abc", b""), Some(0));
}

#[test]
fn needle_longer_than_haystack() {
    assert_eq!(find_sub(b"abc", b"abcd"), None);
    assert_eq!(find_sub(b"", b"a"), None);
}

#[test]
fn one_byte_needle() {
    assert_eq!(find_sub(b"abcabc", b"c"), Some(2));
    assert_eq!(find_sub(b"abc", b"z"), None);
    assert_eq!(find_sub(&[0x00, 0x80, 0xff], &[0x80]), Some(1));
}

#[test]
fn two_three_four_byte_needles() {
    assert_eq!(find_sub(b"xxabyy", b"ab"), Some(2));
    assert_eq!(find_sub(b"xxabcyy", b"abc"), Some(2));
    assert_eq!(find_sub(b"xxabcdyy", b"abcd"), Some(2));
    assert_eq!(find_sub(b"abababx", b"abab"), Some(0));
    assert_eq!(find_sub(b"xabababy", b"abab"), Some(1));
}

#[test]
fn long_needle_periodic() {
    let haystack = b"..abcabcabcabcabc..";
    assert_eq!(find_sub(haystack, b"abcabcabcabc"), Some(2));
    assert_eq!(find_sub(haystack, b"abcabcabcabd"), None);
    let long = vec![b'a'; 300];
    let mut haystack = vec![b'x'; 10_000];
    for (offset, &byte) in long.iter().enumerate() {
        haystack[8_000 + offset] = byte;
    }
    assert_eq!(find_sub(&haystack, &long), Some(8_000));
}

#[test]
fn embedded_nul_is_data() {
    assert_eq!(find_sub(b"a\0b\0c", b"\0b"), Some(1));
    assert_eq!(find_sub(b"\0\0\0", b"\0\0"), Some(0));
    assert_eq!(find_sub(&[0x00, 0xff], &[0x00, 0xff]), Some(0));
}

#[test]
fn high_bytes() {
    let haystack = [0x7f, 0x80, 0xff, 0x80, 0xff, 0x00];
    assert_eq!(find_sub(&haystack, &[0x80, 0xff]), Some(1));
    assert_eq!(find_sub(&haystack, &[0xff, 0x80]), Some(2));
    assert_eq!(find_sub(&haystack, &[0x80, 0xfe]), None);
}

#[test]
fn random_cases_against_naive() {
    let mut state = 0x0bad_cafe_dead_beefu64;
    for _ in 0..20_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let haystack_len = (state as usize) % 40;
        let needle_len = ((state >> 8) as usize) % 12;
        let mut haystack = Vec::with_capacity(haystack_len);
        for _ in 0..haystack_len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            haystack.push(match state % 4 {
                0 => 0x00,
                1 => 0x61,
                2 => 0x80,
                _ => 0xff,
            });
        }
        let mut needle = Vec::with_capacity(needle_len);
        for _ in 0..needle_len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            needle.push(match state % 4 {
                0 => 0x00,
                1 => 0x61,
                2 => 0x80,
                _ => 0xff,
            });
        }
        assert_eq!(find_sub(&haystack, &needle), naive(&haystack, &needle));
    }
}
