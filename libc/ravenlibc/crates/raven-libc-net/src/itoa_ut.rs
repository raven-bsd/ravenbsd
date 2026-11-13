// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Integer writer tests.

use super::*;

#[test]
fn decimal_values() {
    let mut buffer = [0u8; 12];
    assert_eq!(decimal(0, &mut buffer), Some(&b"0"[..]));
    assert_eq!(decimal(1, &mut buffer), Some(&b"1"[..]));
    assert_eq!(decimal(42, &mut buffer), Some(&b"42"[..]));
    assert_eq!(decimal(65_535, &mut buffer), Some(&b"65535"[..]));
    assert_eq!(decimal(u32::MAX, &mut buffer), Some(&b"4294967295"[..]));
}

#[test]
fn decimal_short_buffer() {
    let mut buffer = [0u8; 2];
    assert_eq!(decimal(123, &mut buffer), None);
    assert_eq!(decimal(12, &mut buffer), Some(&b"12"[..]));
}

#[test]
fn ipv4_values() {
    let mut buffer = [0u8; 24];
    assert_eq!(ipv4([10, 0, 0, 2], &mut buffer), Some(&b"10.0.0.2"[..]));
    assert_eq!(
        ipv4([255, 255, 255, 255], &mut buffer),
        Some(&b"255.255.255.255"[..])
    );
    assert_eq!(ipv4([0, 0, 0, 0], &mut buffer), Some(&b"0.0.0.0"[..]));
}

#[test]
fn ipv4_short_buffer() {
    let mut buffer = [0u8; 3];
    assert_eq!(ipv4([192, 0, 2, 1], &mut buffer), None);
}
