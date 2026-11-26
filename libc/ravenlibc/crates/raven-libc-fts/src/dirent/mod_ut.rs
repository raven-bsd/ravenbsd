// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests for the public FreeBSD `struct dirent` record parser.
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
    let reclen = (24 + name.len() + 1 + 7) & !7;
    let mut buf = vec![0u8; reclen];
    buf[0..8].copy_from_slice(&ino.to_le_bytes());
    buf[8..16].copy_from_slice(&off.to_le_bytes());
    buf[16..18].copy_from_slice(&(reclen as u16).to_le_bytes());
    buf[18] = typ;
    buf[19] = 0;
    buf[20..22].copy_from_slice(&(name.len() as u16).to_le_bytes());
    buf[22..24].copy_from_slice(&0u16.to_le_bytes());
    buf[24..24 + name.len()].copy_from_slice(name);
    buf
}

#[test]
fn parses_the_public_layout() {
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
    assert!(parse_dirent(&[0u8; 23], 0).is_err());
    assert!(parse_dirent(&[0u8; 24], 0).is_err()); // reclen 0
}

#[test]
fn reclen_below_32_is_eio() {
    let mut buf = record(1, 1, b"x", 8);
    buf[16..18].copy_from_slice(&31u16.to_le_bytes());
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
    // The NUL that terminates the name sits at 24 + d_namlen.
    buf[24 + 4] = b'x';
    assert!(parse_dirent(&buf, 0).is_err());
}

#[test]
fn namlen_past_the_record_is_eio() {
    let mut buf = record(1, 1, b"x", 8);
    buf[20..22].copy_from_slice(&200u16.to_le_bytes());
    assert!(parse_dirent(&buf, 0).is_err());
}

#[test]
fn an_embedded_nul_is_eio() {
    let mut buf = record(1, 1, b"ab", 8);
    buf[24] = 0;
    assert!(parse_dirent(&buf, 0).is_err());
}

#[test]
fn a_reclen_that_is_not_a_multiple_of_eight_is_accepted() {
    let mut buf = vec![0u8; 33];
    buf[0..8].copy_from_slice(&9u64.to_le_bytes());
    buf[8..16].copy_from_slice(&3i64.to_le_bytes());
    buf[16..18].copy_from_slice(&33u16.to_le_bytes());
    buf[18] = 10;
    buf[20..22].copy_from_slice(&1u16.to_le_bytes());
    buf[24] = b'n';
    let (entry, next) = parse_dirent(&buf, 0).unwrap();
    assert_eq!(entry.name, b"n");
    assert_eq!(next, 33);
}

#[test]
fn position_arithmetic_is_checked() {
    assert!(parse_dirent(&[0u8; 32], usize::MAX).is_err());
}

#[test]
fn a_255_byte_name_parses() {
    let name = vec![b'a'; 255];
    let buf = record(5, 6, &name, 8);
    let (entry, next) = parse_dirent(&buf, 0).unwrap();
    assert_eq!(entry.name.len(), 255);
    assert_eq!(next, buf.len());
    assert_eq!(next, 280);
}

#[test]
fn an_empty_name_is_eio() {
    let mut buf = record(5, 6, b"x", 8);
    buf[20..22].copy_from_slice(&0u16.to_le_bytes());
    assert!(parse_dirent(&buf, 0).is_err());
}

#[test]
fn errors_are_eio() {
    let errno = parse_dirent(&[0u8; 23], 0).unwrap_err();
    assert_eq!(errno.0, raven_libc_abi::generated::errno::EIO as u16);
}
