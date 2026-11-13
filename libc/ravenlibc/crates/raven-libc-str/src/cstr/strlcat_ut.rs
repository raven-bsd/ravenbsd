// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::strlcat;

#[test]
fn every_dsize_dst_len_and_src_len() {
    for dst_len in 0usize..=5 {
        for dsize in 0usize..=8 {
            for src_len in 0usize..=6 {
                if dst_len > dsize {
                    continue;
                }
                let mut dst = vec![b'x'; dsize];
                let mut initial = vec![b'a'; dst_len];
                for (index, slot) in initial.iter_mut().enumerate() {
                    *slot = b'a' + (index as u8 % 20);
                }
                for (index, byte) in initial.iter().enumerate() {
                    dst[index] = *byte;
                }
                let had_nul = dst_len < dsize;
                if had_nul {
                    dst[dst_len] = 0;
                }
                let src: Vec<u8> = (0..src_len).map(|index| b'A' + index as u8).collect();
                let returned = strlcat(&mut dst, &src);
                let expected = if had_nul { dst_len } else { dsize } + src_len;
                assert_eq!(
                    returned, expected,
                    "dsize {dsize} dst_len {dst_len} src_len {src_len}"
                );
                if !had_nul {
                    let mut expected = vec![b'x'; dsize];
                    for (index, byte) in initial.iter().enumerate() {
                        expected[index] = *byte;
                    }
                    assert_eq!(dst, expected);
                } else {
                    let space = dsize - dst_len - 1;
                    let copied = if src_len < space { src_len } else { space };
                    for index in 0..copied {
                        assert_eq!(dst[dst_len + index], src[index]);
                    }
                    assert_eq!(dst[dst_len + copied], 0);
                }
            }
        }
    }
}

#[test]
fn empty_source_keeps_dst() {
    let mut dst = [0xAAu8; 6];
    dst[..3].copy_from_slice(b"ab\0");
    assert_eq!(strlcat(&mut dst, b""), 2);
    assert_eq!(&dst[..3], b"ab\0");
    assert_eq!(dst[3], 0xAA);
}
