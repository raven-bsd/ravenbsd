// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::strlcpy;

#[test]
fn every_dsize_and_source_length() {
    for src_len in 0usize..=8 {
        let src: Vec<u8> = (0..src_len).map(|index| b'a' + index as u8).collect();
        for dsize in 0usize..=10 {
            let mut dst = vec![0xAAu8; dsize];
            let returned = strlcpy(&mut dst, &src);
            assert_eq!(returned, src_len, "src {src_len} dsize {dsize}");
            if dsize == 0 {
                assert!(dst.is_empty());
                continue;
            }
            let copied = if src_len < dsize - 1 {
                src_len
            } else {
                dsize - 1
            };
            assert_eq!(
                &dst[..copied],
                &src[..copied],
                "src {src_len} dsize {dsize}"
            );
            assert_eq!(dst[copied], 0, "src {src_len} dsize {dsize}");
            for (guard, &byte) in dst
                .iter()
                .enumerate()
                .skip(copied + 1)
                .take(dsize.saturating_sub(copied + 1))
            {
                assert_eq!(byte, 0xAA, "src {src_len} dsize {dsize} guard {guard}");
            }
        }
    }
}

#[test]
fn embedded_nul_ends_the_string() {
    let mut dst = [0xAAu8; 6];
    assert_eq!(strlcpy(&mut dst, b"ab\0cdef"), 2);
    assert_eq!(&dst[..3], b"ab\0");
    assert_eq!(dst[3], 0xAA);
}

#[test]
fn exact_fit_needs_one_more_byte() {
    let mut dst = [0xAAu8; 4];
    assert_eq!(strlcpy(&mut dst, b"abcd"), 4);
    assert_eq!(&dst, b"abc\0");
}
