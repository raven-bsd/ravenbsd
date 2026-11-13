// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::{write_i64, write_u64};

fn text(buf: &[u8], len: usize) -> &[u8] {
    &buf[..len]
}

#[test]
fn zero() {
    let mut buf = [0u8; 21];
    let n = write_u64(&mut buf, 0);
    assert_eq!(text(&buf, n), b"0");
}

#[test]
fn small_numbers() {
    for value in 0u64..=2000 {
        let mut buf = [0u8; 21];
        let n = write_u64(&mut buf, value);
        assert_eq!(text(&buf, n), value.to_string().as_bytes(), "value {value}");
    }
}

#[test]
fn u64_boundaries() {
    for value in [u64::MAX, u64::MAX - 1, 1 << 63, (1 << 63) - 1] {
        let mut buf = [0u8; 21];
        let n = write_u64(&mut buf, value);
        assert_eq!(text(&buf, n), value.to_string().as_bytes(), "value {value}");
    }
}

#[test]
fn i64_boundaries() {
    for value in [0i64, 1, -1, 9, -9, i64::MAX, i64::MIN, i64::MIN + 1] {
        let mut buf = [0u8; 21];
        let n = write_i64(&mut buf, value);
        assert_eq!(text(&buf, n), value.to_string().as_bytes(), "value {value}");
    }
}

#[test]
fn one_hundred_thousand_values() {
    let mut state = 0x1234_5678_9abc_def0u64;
    for _ in 0..100_000 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let value = state as i64;
        let mut buf = [0u8; 21];
        let n = write_i64(&mut buf, value);
        assert_eq!(text(&buf, n), value.to_string().as_bytes(), "value {value}");
    }
}

#[test]
fn truncates_instead_of_panicking() {
    let mut buf = [0u8; 3];
    let n = write_u64(&mut buf, 12345);
    assert_eq!(n, 3);
    assert_eq!(&buf, b"123");
    let mut empty = [0u8; 0];
    assert_eq!(write_u64(&mut empty, 7), 0);
    assert_eq!(write_i64(&mut empty, -7), 0);
}
