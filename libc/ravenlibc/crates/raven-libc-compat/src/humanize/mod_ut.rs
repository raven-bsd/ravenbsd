// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `humanize_number`, `expand_number`, `fmt_scaled` and `scan_scaled` tests.
//!
//! The rows are transcribed from `freebsd:lib/libutil/tests/humanize_number_test.c`,
//! `freebsd:lib/libutil/tests/expand_number_test.c` and the OpenSSH copy of
//! `openbsd:lib/libutil/fmt_scaled.c` (built and run by the C65 host probe).

#![allow(
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented,
    clippy::byte_char_slices,
    clippy::field_reassign_with_default
)]

use super::*;
use alloc::vec::Vec;
use raven_libc_abi::errno::Errno;

fn humanize(quotient: i64, suffix: &[u8], scale: i32, flags: i32, len: usize) -> (i32, Vec<u8>) {
    let mut buffer = alloc::vec![b'X'; len];
    let result = humanize_number(&mut buffer, quotient, suffix, scale, flags);
    let written = if result < 0 {
        0
    } else {
        core::cmp::min(usize::try_from(result).unwrap_or(0), len)
    };
    (result, buffer[..written].to_vec())
}

fn expect(quotient: i64, scale: i32, flags: i32, len: usize, result: i32, text: &[u8]) {
    let (got_result, got) = humanize(quotient, b"", scale, flags, len);
    assert_eq!(
        got_result, result,
        "quotient {quotient} scale {scale} flags {flags} len {len}: text {got:?}"
    );
    assert_eq!(
        got, text,
        "quotient {quotient} scale {scale} flags {flags} len {len}"
    );
}

#[test]
fn divisor_1000_autoscale_rows() {
    expect(0, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 2, b"0 ");
    expect(500, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 3, b"1 k");
    expect(500_000, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 3, b"1 M");
    expect(500_000_000, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 3, b"1 G");
    expect(500_000_000_000, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 3, b"1 T");
    expect(
        500_000_000_000_000,
        HN_AUTOSCALE,
        HN_DIVISOR_1000,
        4,
        3,
        b"1 P",
    );
    expect(
        500_000_000_000_000_000,
        HN_AUTOSCALE,
        HN_DIVISOR_1000,
        4,
        3,
        b"1 E",
    );
    expect(1, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 2, b"1 ");
    expect(1500, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 3, b"2 k");
    expect(1_500_000, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 3, b"2 M");
    expect(1_500_000_000, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 3, b"2 G");
    expect(
        1_500_000_000_000,
        HN_AUTOSCALE,
        HN_DIVISOR_1000,
        4,
        3,
        b"2 T",
    );
    expect(
        1_500_000_000_000_000,
        HN_AUTOSCALE,
        HN_DIVISOR_1000,
        4,
        3,
        b"2 P",
    );
    expect(
        1_500_000_000_000_000_000,
        HN_AUTOSCALE,
        HN_DIVISOR_1000,
        4,
        3,
        b"2 E",
    );
}

#[test]
fn divisor_1024_autoscale_rows() {
    expect(0, HN_AUTOSCALE, 0, 4, 2, b"0 ");
    expect(512, HN_AUTOSCALE, 0, 4, 3, b"1 K");
    expect(512 * 1024, HN_AUTOSCALE, 0, 4, 3, b"1 M");
    expect(512 * 1024 * 1024, HN_AUTOSCALE, 0, 4, 3, b"1 G");
    expect(512 * 1024_i64.pow(3), HN_AUTOSCALE, 0, 4, 3, b"1 T");
    expect(512 * 1024_i64.pow(4), HN_AUTOSCALE, 0, 4, 3, b"1 P");
    expect(512 * 1024_i64.pow(5), HN_AUTOSCALE, 0, 4, 3, b"1 E");
    expect(1, HN_AUTOSCALE, 0, 4, 2, b"1 ");
    expect(1536, HN_AUTOSCALE, 0, 4, 3, b"2 K");
    expect(1536 * 1024, HN_AUTOSCALE, 0, 4, 3, b"2 M");
    expect(1536 * 1024 * 1024, HN_AUTOSCALE, 0, 4, 3, b"2 G");
    expect(1536 * 1024_i64.pow(3), HN_AUTOSCALE, 0, 4, 3, b"2 T");
    expect(1536 * 1024_i64.pow(4), HN_AUTOSCALE, 0, 4, 3, b"2 P");
    expect(1536 * 1024_i64.pow(5), HN_AUTOSCALE, 0, 4, 3, b"2 E");
}

#[test]
fn rounding_rows() {
    expect(500_000 - 1, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 3, b"0 M");
    expect(500_000, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 3, b"1 M");
    expect(1_500_000 - 1, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 3, b"1 M");
    expect(1_500_000, HN_AUTOSCALE, HN_DIVISOR_1000, 4, 3, b"2 M");
    expect(511, HN_AUTOSCALE, 0, 4, 3, b"0 K");
    expect(512, HN_AUTOSCALE, 0, 4, 3, b"1 K");
    expect(512 * 1024 - 1, HN_AUTOSCALE, 0, 4, 3, b"0 M");
    expect(512 * 1024, HN_AUTOSCALE, 0, 4, 3, b"1 M");
}

#[test]
fn specific_scale_rows() {
    expect(0, 1, HN_DIVISOR_1000, 4, 3, b"0 k");
    expect(500, 1, HN_DIVISOR_1000, 4, 3, b"1 k");
    expect(500, 2, HN_DIVISOR_1000, 4, 3, b"0 M");
    expect(500_000, 2, HN_DIVISOR_1000, 4, 3, b"1 M");
    expect(1_500, 1, HN_DIVISOR_1000, 4, 3, b"2 k");
    expect(0, 1, 0, 4, 3, b"0 K");
    expect(512, 1, 0, 4, 3, b"1 K");
    expect(1536, 1, 0, 4, 3, b"2 K");
}

#[test]
fn invalid_scales_are_minus_one() {
    for scale in [7, 1000, 1_000_000, i32::MAX, -1, -1000, -i32::MAX] {
        expect(1, scale, 0, 4, -1, b"");
        expect(1, scale, HN_DIVISOR_1000, 4, -1, b"");
    }
}

#[test]
fn scale_zero_prints_the_whole_number() {
    expect(0, 0, HN_DIVISOR_1000, 4, 2, b"0 ");
    expect(1, 0, HN_DIVISOR_1000, 4, 2, b"1 ");
    expect(10, 0, HN_DIVISOR_1000, 4, 3, b"10 ");
    expect(150, 0, HN_DIVISOR_1000, 4, 4, b"150 ");
    expect(999, 0, 0, 4, 4, b"999 ");
    expect(1000, 0, HN_DIVISOR_1000, 4, 5, b"1000");
}

#[test]
fn decimal_and_iec_rows() {
    expect(1536, 1, HN_DECIMAL, 6, 5, b"1.5 K");
    expect(1536, HN_AUTOSCALE, HN_DECIMAL | HN_NOSPACE, 4, 4, b"1.5K");
    expect(
        1536,
        HN_AUTOSCALE,
        HN_DECIMAL | HN_IEC_PREFIXES | HN_NOSPACE,
        5,
        5,
        b"1.5Ki",
    );
    let (result, text) = humanize(
        1536,
        b"B",
        HN_AUTOSCALE,
        HN_DECIMAL | HN_IEC_PREFIXES | HN_B | HN_NOSPACE,
        6,
    );
    assert_eq!(result, 6);
    assert_eq!(text, b"1.5KiB");
    expect(
        1500,
        HN_AUTOSCALE,
        HN_DECIMAL | HN_B | HN_DIVISOR_1000 | HN_NOSPACE,
        5,
        4,
        b"1.5k",
    );
}

#[test]
fn getscale_returns_the_index() {
    let mut buffer = [b'X'; 4];
    assert_eq!(humanize_number(&mut buffer, 1536, b"", HN_GETSCALE, 0), 1);
    let mut buffer = [b'X'; 4];
    assert_eq!(
        humanize_number(&mut buffer, 1536 * 1024, b"", HN_GETSCALE, 0),
        2
    );
    let mut buffer = [b'X'; 8];
    assert_eq!(humanize_number(&mut buffer, 0, b"", HN_GETSCALE, 0), 0);
}

#[test]
fn negatives_and_suffixes() {
    expect(-1536, 1, HN_DECIMAL, 8, 6, b"-1.5 K");
    expect(-1, HN_AUTOSCALE, 0, 8, 3, b"-1 ");
    let (result, text) = humanize(1024, b"B", 1, 0, 8);
    assert_eq!(result, 4);
    assert_eq!(text, b"1 KB");
}

#[test]
fn too_small_buffer_is_minus_one() {
    expect(1, 0, 0, 2, -1, b"");
    expect(500, HN_AUTOSCALE, HN_DIVISOR_1000, 2, -1, b"");
}

#[test]
fn conflicting_flags_are_minus_one() {
    expect(
        1,
        HN_AUTOSCALE,
        HN_DIVISOR_1000 | HN_IEC_PREFIXES,
        8,
        -1,
        b"",
    );
}

#[test]
fn expand_number_accepts_the_donor_rows() {
    let ok: &[(&[u8], i64)] = &[
        (b"-0", 0),
        (b" 0", 0),
        (b"+0", 0),
        (b"-1", -1),
        (b" 1", 1),
        (b"+1", 1),
        (b"-10", -10),
        (b"1B", 1),
        (b"1K", 1024),
        (b"1M", 1 << 20),
        (b"1G", 1 << 30),
        (b"1T", 1 << 40),
        (b"1P", 1 << 50),
        (b"1E", 1 << 60),
        (b"2b", 2),
        (b"2k", 2048),
        (b"2m", 2 << 20),
        (b"2g", 2 << 30),
        (b"2t", 2 << 40),
        (b"2p", 2 << 50),
        (b"2e", 2 << 60),
        (b"3KB", 3 << 10),
        (b"3MB", 3 << 20),
        (b"3GB", 3 << 30),
        (b"3EB", 3 << 60),
        (b"-1K", -1024),
        (b"-3EB", -(3 << 60)),
        (b"7E", 7 << 60),
        (b"8191P", 8191 << 50),
        (b"9223372036854775807", i64::MAX),
        (b"-9223372036854775808", i64::MIN),
    ];
    for (text, value) in ok {
        assert_eq!(expand_number(text), Ok(*value), "text {text:?}");
    }
}

#[test]
fn expand_number_error_rows() {
    for text in [
        b"".as_slice(),
        b"b",
        b"k",
        b"-",
        b"-b",
        b"1x",
        b"1KK",
        b"1 2",
        b"1K2",
        b"  ",
        b"++1",
        b"--1",
        b"1.5K",
        b"08",
    ] {
        assert_eq!(
            expand_number(text),
            Err(Errno(raven_libc_abi::errno::EINVAL as u16)),
            "text {text:?}"
        );
    }
    for text in [
        b"9223372036854775808".as_slice(),
        b"8E",
        b"-9223372036854775809",
    ] {
        assert_eq!(
            expand_number(text),
            Err(Errno(raven_libc_abi::errno::ERANGE as u16)),
            "text {text:?}"
        );
    }
}

#[test]
fn base_prefixes() {
    assert_eq!(expand_number(b"010"), Ok(8));
    assert_eq!(expand_number(b"0x10"), Ok(16));
    assert_eq!(expand_number(b"0X1f"), Ok(31));
}

#[test]
fn fmt_scaled_donor_rows() {
    let rows: &[(i64, &[u8])] = &[
        (0, b"0B"),
        (1, b"1B"),
        (512, b"512B"),
        (1024, b"1.0K"),
        (1536, b"1.5K"),
        (10_485_760, b"10.0M"),
        (-1024, b"-1.0K"),
        (1024_i64.pow(3), b"1.0G"),
        (1024_i64.pow(4), b"1.0T"),
        (1024_i64.pow(5), b"1.0P"),
        (1024_i64.pow(6) - 1, b"1024P"),
        (1024_i64.pow(6), b"1.0E"),
    ];
    for (number, expected) in rows {
        let mut buffer = [0u8; FMT_SCALED_STRSIZE];
        fmt_scaled(*number, &mut buffer).unwrap();
        assert_eq!(&buffer[..expected.len()], *expected, "number {number}");
    }
}

#[test]
fn fmt_scaled_range_error() {
    let mut buffer = [0u8; FMT_SCALED_STRSIZE];
    assert!(fmt_scaled(i64::MAX, &mut buffer).is_ok());
    assert_eq!(
        fmt_scaled(i64::MIN, &mut buffer),
        Err(Errno(raven_libc_abi::errno::ERANGE as u16))
    );
}

#[test]
fn scan_scaled_rows() {
    let rows: &[(&[u8], i64)] = &[
        (b"1", 1),
        (b"1B", 1),
        (b"1K", 1024),
        (b"1k", 1024),
        (b"1.5K", 1536),
        (b"0.5K", 512),
        (b"-1K", -1024),
        (b"+1K", 1024),
        (b" 1M", 1024 * 1024),
        (b"1024P", 1024_i64.pow(6)),
        (b"0", 0),
        (b"1e", 1024_i64.pow(6)),
    ];
    for (text, value) in rows {
        assert_eq!(scan_scaled(text), Ok(*value), "text {text:?}");
    }
}

#[test]
fn scan_scaled_errors() {
    for text in [b"1.2.3".as_slice(), b"1K2", b"--1", b"+-1"] {
        assert_eq!(
            scan_scaled(text),
            Err(Errno(raven_libc_abi::errno::EINVAL as u16)),
            "text {text:?}"
        );
    }
    assert_eq!(
        scan_scaled(b"1Q"),
        Err(Errno(raven_libc_abi::errno::ERANGE as u16))
    );
}

#[test]
fn fmt_scan_roundtrip_for_seeded_values() {
    let mut state: u64 = 0x1234_5678_9abc_def0;
    for _ in 0..1000 {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let value = (state.wrapping_mul(0x2545_f491_4f6c_dd1d) % 1_000_000_000) as i64;
        let mut buffer = [0u8; FMT_SCALED_STRSIZE];
        fmt_scaled(value, &mut buffer).unwrap();
        let end = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
        let scanned = scan_scaled(&buffer[..end]).unwrap();
        let relative = (scanned - value).abs();
        let tolerance = if value < 1024 { 1 } else { value / 20 + 1024 };
        assert!(relative <= tolerance, "value {value} scanned {scanned}");
    }
}
