// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::strverscmp;

fn check_all(ordered: &[&[u8]]) {
    for (i, left) in ordered.iter().enumerate() {
        for (j, right) in ordered.iter().enumerate() {
            let value = strverscmp(left, right);
            if i == j {
                assert_eq!(value, 0, "{left:?} == {right:?}");
            } else if i < j {
                assert!(value < 0, "{left:?} < {right:?} (got {value})");
            } else {
                assert!(value > 0, "{left:?} > {right:?} (got {value})");
            }
        }
    }
}

#[test]
fn plain_strings() {
    check_all(&[b"", b"a", b"b"]);
}

#[test]
fn freebsd_vers_ordering() {
    check_all(&[b"000", b"00", b"01", b"010", b"09", b"0", b"1", b"9", b"10"]);
}

#[test]
fn freebsd_natural_ordering() {
    check_all(&[
        b"jan1", b"jan2", b"jan9", b"jan10", b"jan11", b"jan19", b"jan20",
    ]);
}

#[test]
fn freebsd_glibc_bug_9913() {
    check_all(&[
        b"B0075022800016.gbp.corp.com",
        b"B007502280067.gbp.corp.com",
        b"B007502357019.GBP.CORP.COM",
    ]);
}

#[test]
fn freebsd_semver_ordering() {
    check_all(&[b"2.6.20", b"2.6.21"]);
}

#[test]
fn ticket_vectors() {
    check_all(&[b"000", b"0.0.1", b"1.2.9", b"1.2.10", b"a00", b"a0", b"a1"]);
}

#[test]
fn identical_content_is_zero() {
    assert_eq!(strverscmp(b"abc", b"abc"), 0);
    assert_eq!(strverscmp(b"", b""), 0);
    assert_eq!(strverscmp(b"1.0", b"1.0"), 0);
}

#[test]
fn non_digit_disagreement_is_byte_order() {
    assert!(strverscmp(b"a", b"b") < 0);
    assert!(strverscmp(b"B", b"a") < 0);
    assert!(strverscmp(b"\x80", b"\x7f") > 0);
}

#[test]
fn random_antisymmetry() {
    let mut state = 0xfeed_face_cafe_beefu64;
    for _ in 0..20_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let len = (state as usize) % 8;
        let mut left = Vec::with_capacity(len);
        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            left.push(match state % 12 {
                10 => b'.',
                11 => b'-',
                value => b'0' + value as u8,
            });
        }
        let len = ((state >> 16) as usize) % 8;
        let mut right = Vec::with_capacity(len);
        for _ in 0..len {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            right.push(match state % 12 {
                10 => b'.',
                11 => b'-',
                value => b'0' + value as u8,
            });
        }
        let forward = strverscmp(&left, &right);
        let backward = strverscmp(&right, &left);
        assert_eq!(forward.signum(), -backward.signum(), "{left:?} {right:?}");
    }
}
