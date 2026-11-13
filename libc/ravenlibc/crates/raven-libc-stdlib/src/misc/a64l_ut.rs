// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! `a64l` vectors from NetBSD `t_a64l.c` and the exhaustive length <= 3 domain.

use super::*;
use crate::misc::l64a;

#[test]
fn netbsd_vectors() {
    assert_eq!(a64l(b""), Ok(0));
    assert_eq!(a64l(b"///"), Ok(4161));
    assert_eq!(a64l(b"123"), Ok(20_739));
    assert_eq!(a64l(b"zzz"), Ok(262_143));
    assert_eq!(a64l(b"a64l"), Ok(12_870_182));
    assert_eq!(a64l(b"a64l..."), Ok(12_870_182));
}

#[test]
fn l64a_vectors() {
    let mut out = [0u8; 8];
    l64a(0, &mut out);
    assert_eq!(out[0], 0);
    l64a(4161, &mut out);
    assert_eq!(&out[..3], b"///");
    l64a(20_739, &mut out);
    assert_eq!(&out[..3], b"123");
    l64a(262_143, &mut out);
    assert_eq!(&out[..3], b"zzz");
    l64a(12_870_182, &mut out);
    assert_eq!(&out[..4], b"a64l");
}

#[test]
fn invalid_characters_are_einval() {
    for text in [b"#".as_slice(), b"a!", b" a"] {
        assert_eq!(
            a64l(text),
            Err(raven_libc_abi::errno::Errno(
                raven_libc_abi::errno::EINVAL as u16
            ))
        );
    }
}

#[test]
fn exhaustive_length_three() {
    let digits = b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    for length in 0..=3usize {
        let total = digits.len().pow(length as u32);
        for number in 0..total {
            let mut value = number;
            let mut text = Vec::new();
            let mut expected = 0i64;
            let mut shift = 0u32;
            for _ in 0..length {
                let digit = value % digits.len();
                value /= digits.len();
                text.push(digits[digit]);
                expected += (digit as i64) << shift;
                shift += 6;
            }
            assert_eq!(a64l(&text), Ok(expected), "{text:?}");
        }
    }
}

#[test]
fn l64a_negative_is_empty() {
    let mut out = [9u8; 8];
    l64a(-1, &mut out);
    assert_eq!(out[0], 0);
}

#[test]
fn six_characters_maximum() {
    assert_eq!(a64l(b"zzzzzzz"), Ok(68_719_476_735));
}

#[test]
fn nul_stops_the_scan() {
    assert_eq!(a64l(b"a\0zz"), Ok(38));
}

#[test]
fn all_digit_boundaries() {
    assert_eq!(a64l(b"."), Ok(0));
    assert_eq!(a64l(b"/"), Ok(1));
    assert_eq!(a64l(b"0"), Ok(2));
    assert_eq!(a64l(b"9"), Ok(11));
    assert_eq!(a64l(b"A"), Ok(12));
    assert_eq!(a64l(b"Z"), Ok(37));
    assert_eq!(a64l(b"a"), Ok(38));
    assert_eq!(a64l(b"z"), Ok(63));
}
