// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the Linux `dirent64` record parser.
// Test-only code is exempt from the production lints (17-crate-layout 9.3).
#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use super::*;

fn record(ino: u64, off: i64, name: &[u8], typ: u8) -> Vec<u8> {
    let reclen = (19 + name.len() + 1 + 7) & !7;
    let mut buf = vec![0u8; reclen];
    buf[0..8].copy_from_slice(&ino.to_le_bytes());
    buf[8..16].copy_from_slice(&off.to_le_bytes());
    buf[16..18].copy_from_slice(&(reclen as u16).to_le_bytes());
    buf[18] = typ;
    buf[19..19 + name.len()].copy_from_slice(name);
    buf
}

#[test]
fn parses_the_linux_layout() {
    let buf = record(42, 7, b"hello", 8);
    let (entry, next) = parse_dirent(&buf, 0).unwrap();
    assert_eq!(entry.ino, 42);
    assert_eq!(entry.off, 7);
    assert_eq!(entry.typ, 8);
    assert_eq!(entry.name, b"hello");
    assert_eq!(entry.reclen as usize, buf.len());
    assert_eq!(next, buf.len());
}

#[test]
fn parses_at_a_nonzero_position() {
    let mut buf = record(1, 1, b"a", 4);
    let second = record(2, 2, b"bb", 8);
    let start = buf.len();
    buf.extend_from_slice(&second);
    let (entry, next) = parse_dirent(&buf, start).unwrap();
    assert_eq!(entry.ino, 2);
    assert_eq!(entry.name, b"bb");
    assert_eq!(next, buf.len());
}

#[test]
fn truncated_header_is_eio() {
    assert!(parse_dirent(&[], 0).is_err());
    assert!(parse_dirent(&[0u8; 19], 0).is_err());
    assert!(parse_dirent(&[0u8; 20], 0).is_err()); // reclen 0
}

#[test]
fn reclen_below_20_is_eio() {
    let mut buf = record(1, 1, b"x", 8);
    buf[16..18].copy_from_slice(&19u16.to_le_bytes());
    assert!(parse_dirent(&buf, 0).is_err());
    buf[16..18].copy_from_slice(&0u16.to_le_bytes());
    assert!(parse_dirent(&buf, 0).is_err());
}

#[test]
fn reclen_past_the_buffer_is_eio() {
    let mut buf = record(1, 1, b"x", 8);
    let too_big = (buf.len() + 8) as u16;
    buf[16..18].copy_from_slice(&too_big.to_le_bytes());
    assert!(parse_dirent(&buf, 0).is_err());
}

#[test]
fn name_without_a_nul_is_eio() {
    let mut buf = record(1, 1, b"abcd", 8);
    let len = buf.len();
    for byte in &mut buf[19..len] {
        *byte = b'x';
    }
    assert!(parse_dirent(&buf, 0).is_err());
}

#[test]
fn a_reclen_that_is_not_a_multiple_of_eight_is_accepted() {
    let mut buf = vec![0u8; 21];
    buf[0..8].copy_from_slice(&9u64.to_le_bytes());
    buf[8..16].copy_from_slice(&3i64.to_le_bytes());
    buf[16..18].copy_from_slice(&21u16.to_le_bytes());
    buf[18] = 10;
    buf[19] = b'n';
    let (entry, next) = parse_dirent(&buf, 0).unwrap();
    assert_eq!(entry.name, b"n");
    assert_eq!(next, 21);
}

#[test]
fn position_arithmetic_is_checked() {
    assert!(parse_dirent(&[0u8; 24], usize::MAX).is_err());
}

#[test]
fn a_255_byte_name_parses() {
    let name = vec![b'a'; 255];
    let buf = record(5, 6, &name, 8);
    let (entry, next) = parse_dirent(&buf, 0).unwrap();
    assert_eq!(entry.name.len(), 255);
    assert_eq!(next, buf.len());
}

#[test]
fn an_empty_name_is_valid() {
    let buf = record(5, 6, b"", 8);
    let (entry, _next) = parse_dirent(&buf, 0).unwrap();
    assert!(entry.name.is_empty());
}

#[test]
fn errors_are_eio() {
    let errno = parse_dirent(&[0u8; 19], 0).unwrap_err();
    assert_eq!(errno.0, raven_libc_abi::generated::errno::EIO as u16);
}
