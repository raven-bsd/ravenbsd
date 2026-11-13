// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{Load, bcmp_with, memcmp_with, timingsafe_bcmp, timingsafe_memcmp};

struct Counting<'a> {
    bytes: &'a [u8],
    loads: usize,
}

impl Load for Counting<'_> {
    fn get(&mut self, index: usize) -> u8 {
        self.loads += 1;
        self.bytes[index]
    }
}

#[test]
fn bcmp_equal_and_different() {
    assert_eq!(timingsafe_bcmp(b"", b""), 0);
    assert_eq!(timingsafe_bcmp(b"abc", b"abc"), 0);
    assert_eq!(timingsafe_bcmp(b"abc", b"abd"), 1);
    assert_eq!(timingsafe_bcmp(b"abc", b"xbc"), 1);
}

#[test]
fn bcmp_any_difference_is_one() {
    assert_eq!(timingsafe_bcmp(&[0xff], &[0x00]), 1);
    assert_eq!(timingsafe_bcmp(&[0x80], &[0x7f]), 1);
    assert_eq!(timingsafe_bcmp(&[0x01, 0x02], &[0x01, 0x03]), 1);
}

#[test]
fn memcmp_orders() {
    assert_eq!(timingsafe_memcmp(b"", b""), 0);
    assert_eq!(timingsafe_memcmp(b"abc", b"abc"), 0);
    assert_eq!(timingsafe_memcmp(b"abc", b"abd"), -1);
    assert_eq!(timingsafe_memcmp(b"abd", b"abc"), 1);
    assert_eq!(timingsafe_memcmp(&[0x00], &[0xff]), -1);
    assert_eq!(timingsafe_memcmp(&[0xff], &[0x00]), 1);
    assert_eq!(timingsafe_memcmp(&[0x80], &[0x7f]), 1);
}

#[test]
fn memcmp_first_difference_wins() {
    assert_eq!(timingsafe_memcmp(b"aXc", b"aYd"), -1);
    assert_eq!(timingsafe_memcmp(b"aYc", b"aXd"), 1);
}

#[test]
fn access_count_is_exactly_two_n() {
    let mut state = 0x0123_4567_89ab_cdefu64;
    for case in 0..20_000usize {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let n = (state as usize) % 33;
        let mut left = vec![0u8; n];
        let mut right = vec![0u8; n];
        for index in 0..n {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            left[index] = (state >> 8) as u8;
            right[index] = (state >> 16) as u8;
        }
        if case % 3 == 0 && n != 0 {
            right[0] = left[0] ^ 1;
        }
        if case % 5 == 0 && n > 1 {
            right[n - 1] = !left[n - 1];
        }
        let mut left_loader = Counting {
            bytes: &left,
            loads: 0,
        };
        let mut right_loader = Counting {
            bytes: &right,
            loads: 0,
        };
        let _ = bcmp_with(&mut left_loader, &mut right_loader, n);
        assert_eq!(left_loader.loads, n, "case {case} left");
        assert_eq!(right_loader.loads, n, "case {case} right");

        let mut left_loader = Counting {
            bytes: &left,
            loads: 0,
        };
        let mut right_loader = Counting {
            bytes: &right,
            loads: 0,
        };
        let _ = memcmp_with(&mut left_loader, &mut right_loader, n);
        assert_eq!(left_loader.loads, n, "case {case} memcmp left");
        assert_eq!(right_loader.loads, n, "case {case} memcmp right");
    }
}

#[test]
fn memcmp_is_branch_free_by_construction() {
    // A difference in the first byte must not shorten the trip: the result is still produced by
    // reading both buffers completely.
    let left = [1u8; 64];
    let mut right = [1u8; 64];
    right[0] = 2;
    let mut left_loader = Counting {
        bytes: &left,
        loads: 0,
    };
    let mut right_loader = Counting {
        bytes: &right,
        loads: 0,
    };
    assert_eq!(memcmp_with(&mut left_loader, &mut right_loader, 64), -1);
    assert_eq!(left_loader.loads, 64);
    assert_eq!(right_loader.loads, 64);
}
