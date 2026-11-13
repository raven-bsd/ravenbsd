// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Differential test: every byte 0..=255 x the twelve single-flag sets against the C original
//! (`netbsd:lib/libc/gen/vis.c`), recorded by the C65 host probe in `tests/data/vis_oracle.rs`.
//! The C `vis()` returns a pointer and no length; the probe recorded the pointer delta and the
//! bytes, which is the same contract as `vis(dst, c, flags, nextc) -> Option<usize>`.

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

#[path = "data/vis_oracle.rs"]
mod oracle;

use raven_libc_compat::vis::unvis::strunvis;
use raven_libc_compat::vis::{Unvis, UnvisStep, vis};

/// The two bytes whose wide classification differs between the glibc oracle host and
/// `raven-libc-ctype` (musl tables): U+0085 NEL (a space in musl, not in glibc's locale at
/// this build) and U+00A0 NO-BREAK SPACE (the reverse). They are asserted separately below.
const LOCALE_DIVERGENT: [u8; 1] = [0x85];

fn check(flags: u32, rows: &[(u8, usize, [u8; 8])]) {
    for (byte, length, expected) in rows {
        if LOCALE_DIVERGENT.contains(byte) {
            continue;
        }
        let mut dst = [0u8; 8];
        let got = vis(&mut dst, *byte, flags, b'x').expect("vis");
        assert_eq!(got, *length, "byte {byte:#04x} flags {flags:#06x}");
        assert_eq!(
            &dst[..got],
            &expected[..*length],
            "byte {byte:#04x} flags {flags:#06x}"
        );
    }
}

#[test]
fn every_byte_with_no_flags() {
    check(oracle::NONE, oracle::NONE_ROWS);
}

#[test]
fn every_byte_with_cstyle() {
    check(oracle::CSTYLE, oracle::CSTYLE_ROWS);
}

#[test]
fn every_byte_with_octal() {
    check(oracle::OCTAL, oracle::OCTAL_ROWS);
}

#[test]
fn every_byte_with_white() {
    check(oracle::WHITE, oracle::WHITE_ROWS);
}

#[test]
fn every_byte_with_safe() {
    check(oracle::SAFE, oracle::SAFE_ROWS);
}

#[test]
fn every_byte_with_httpstyle() {
    check(oracle::HTTPSTYLE, oracle::HTTPSTYLE_ROWS);
}

#[test]
fn every_byte_with_mimestyle() {
    check(oracle::MIMESTYLE, oracle::MIMESTYLE_ROWS);
}

#[test]
fn every_byte_with_glob() {
    check(oracle::GLOB, oracle::GLOB_ROWS);
}

#[test]
fn every_byte_with_shell() {
    check(oracle::SHELL, oracle::SHELL_ROWS);
}

#[test]
fn every_byte_with_noslash() {
    check(oracle::NOSLASH, oracle::NOSLASH_ROWS);
}

#[test]
fn every_byte_with_dq() {
    check(oracle::DQ, oracle::DQ_ROWS);
}

#[test]
fn every_byte_with_nolocale() {
    check(oracle::NOLOCALE, oracle::NOLOCALE_ROWS);
}

#[test]
fn unvis_accepts_the_third_octal_digit_and_flushes() {
    let mut machine = Unvis::new();
    assert_eq!(machine.feed(b'\\', 0), UnvisStep::NoChar);
    assert_eq!(machine.feed(b'1', 0), UnvisStep::NoChar);
    assert_eq!(machine.feed(b'0', 0), UnvisStep::NoChar);
    assert_eq!(machine.feed(b'1', 0), UnvisStep::Valid(0o101));
}

#[test]
fn strunvis_rejects_an_overlong_octal() {
    let mut dst = [0u8; 8];
    assert_eq!(strunvis(&mut dst, b"\\477", 0), None);
}

#[test]
fn locale_divergent_bytes_follow_raven_libc_ctype() {
    // U+0085 NEL is a space in musl's iswspace list (glibc's C.UTF-8 says it is not), so it
    // does not pass the graph check and takes the byte escape path.
    let mut dst = [0u8; 8];
    let length = vis(&mut dst, 0x85, 0, b'x').unwrap();
    assert_eq!(&dst[..length], b"\\M^E");
    // U+00A0 matches the glibc oracle: it is graph there and passes through as the raw byte
    // in the one-byte (invalid UTF-8) conversion path.
    let mut dst = [0u8; 8];
    let length = vis(&mut dst, 0xa0, 0, b'x').unwrap();
    assert_eq!(&dst[..length], b"\xa0");
}
