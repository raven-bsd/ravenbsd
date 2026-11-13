// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;
use crate::test_support::RecordingSource;

fn scan(input: &[u8], base: u32, lim: u64) -> (u64, usize) {
    let mut src = RecordingSource::new(input.to_vec());
    let mut f = Limited::new(&mut src);
    f.shlim(0);
    let v = intscan(&mut f, base, false, lim);
    let c = f.count();
    f.finish();
    (v, c)
}

#[test]
fn decimal_values() {
    assert_eq!(scan(b"0", 10, u64::MAX).0, 0);
    assert_eq!(scan(b"1", 10, u64::MAX).0, 1);
    assert_eq!(scan(b"42", 10, u64::MAX).0, 42);
    assert_eq!(scan(b"-42", 10, u64::MAX).0, (-42i64) as u64);
    assert_eq!(scan(b"+42", 10, u64::MAX).0, 42);
    assert_eq!(scan(b"  42", 10, u64::MAX).0, 42);
    assert_eq!(scan(b"00042", 10, u64::MAX).0, 42);
}

#[test]
fn hexadecimal_and_prefixes() {
    assert_eq!(scan(b"0x1f", 16, u64::MAX).0, 31);
    assert_eq!(scan(b"0X1F", 16, u64::MAX).0, 31);
    assert_eq!(scan(b"1f", 16, u64::MAX).0, 31);
    assert_eq!(scan(b"0x1f", 0, u64::MAX).0, 31);
    assert_eq!(scan(b"0", 0, u64::MAX).0, 0);
    assert_eq!(scan(b"0755", 0, u64::MAX).0, 0o755);
    assert_eq!(scan(b"0755", 8, u64::MAX).0, 0o755);
    assert_eq!(scan(b"42", 0, u64::MAX).0, 42);
}

#[test]
fn no_digits_is_zero_and_consumes_nothing() {
    let (v, c) = scan(b"xyz", 10, u64::MAX);
    assert_eq!(v, 0);
    assert_eq!(c, 0);
    let (v, c) = scan(b"", 10, u64::MAX);
    assert_eq!(v, 0);
    assert_eq!(c, 0);
}

#[test]
fn a_bad_hex_prefix_leaves_the_zero() {
    let (v, c) = scan(b"0x", 16, u64::MAX);
    assert_eq!(v, 0);
    assert_eq!(c, 0);
}

#[test]
fn saturation_to_u64_max() {
    assert_eq!(
        scan(b"99999999999999999999999999", 10, u64::MAX).0,
        u64::MAX
    );
    assert_eq!(scan(b"0xffffffffffffffffff", 16, u64::MAX).0, u64::MAX);
    assert_eq!(
        scan(
            b"111111111111111111111111111111111111111111111111111111111111111111111",
            2,
            u64::MAX
        )
        .0,
        u64::MAX
    );
}

#[test]
fn the_limit_comes_from_the_cursor() {
    let mut src = RecordingSource::new(b"12345".to_vec());
    let mut f = Limited::new(&mut src);
    f.shlim(2);
    let v = intscan(&mut f, 10, false, u64::MAX);
    assert_eq!(v, 12);
    assert_eq!(f.count(), 2);
    assert_eq!(f.getc(), Some(b'3'));
}
