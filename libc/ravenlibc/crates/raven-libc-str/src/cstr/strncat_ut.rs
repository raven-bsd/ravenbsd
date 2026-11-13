// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::strncat_len;

#[test]
fn appends_at_most_n() {
    assert_eq!(strncat_len(0, b"abc", 2), 2);
    assert_eq!(strncat_len(0, b"abc", 10), 3);
    assert_eq!(strncat_len(5, b"", 10), 0);
    assert_eq!(strncat_len(0, b"abc", 0), 0);
}

#[test]
fn appends_nul_only() {
    // C writes at most n+1 bytes (the n bytes plus the terminator the caller adds).
    for n in 0usize..=4 {
        assert_eq!(strncat_len(0, b"abcdef", n), n);
    }
}

#[test]
fn every_source_length() {
    for src_len in 0usize..=8 {
        let src: Vec<u8> = vec![b'a'; src_len];
        for n in 0usize..=10 {
            assert_eq!(
                strncat_len(3, &src, n),
                if src_len < n { src_len } else { n }
            );
        }
    }
}
