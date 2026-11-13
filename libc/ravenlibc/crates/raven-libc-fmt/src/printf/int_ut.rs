// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn digits(val: u64, base: u32, octzero: bool, upper: bool) -> String {
    let mut buf = [0u8; 64];
    let start = ultoa(val, &mut buf, base, octzero, upper);
    String::from_utf8(buf[start..].to_vec()).unwrap()
}

#[test]
fn decimal_digits() {
    assert_eq!(digits(0, 10, false, false), "0");
    assert_eq!(digits(1, 10, false, false), "1");
    assert_eq!(digits(9, 10, false, false), "9");
    assert_eq!(digits(10, 10, false, false), "10");
    assert_eq!(digits(i32::MAX as u64, 10, false, false), "2147483647");
    assert_eq!(digits(u32::MAX as u64, 10, false, false), "4294967295");
    assert_eq!(digits(u64::MAX, 10, false, false), "18446744073709551615");
    assert_eq!(
        digits(0x8000_0000_0000_0000, 10, false, false),
        "9223372036854775808"
    );
}

#[test]
fn binary_digits() {
    assert_eq!(digits(0, 2, false, false), "0");
    assert_eq!(digits(5, 2, false, false), "101");
    assert_eq!(digits(255, 2, false, false), "11111111");
    assert_eq!(digits(u64::MAX, 2, false, false).len(), 64);
}

#[test]
fn octal_digits() {
    assert_eq!(digits(0, 8, false, false), "0");
    assert_eq!(digits(0, 8, true, false), "0");
    assert_eq!(digits(8, 8, false, false), "10");
    assert_eq!(digits(8, 8, true, false), "010");
    assert_eq!(digits(0o755, 8, false, false), "755");
    assert_eq!(digits(0o755, 8, true, false), "0755");
}

#[test]
fn hex_digits_case() {
    assert_eq!(digits(0, 16, false, false), "0");
    assert_eq!(digits(0xdead_beef, 16, false, false), "deadbeef");
    assert_eq!(digits(0xdead_beef, 16, false, true), "DEADBEEF");
}

#[test]
fn exponent_formats() {
    let mut buf = [0u8; 8];
    let n = exponent(&mut buf, 1, b'e');
    assert_eq!(&buf[..n], b"e+01");
    let n = exponent(&mut buf, -12, b'e');
    assert_eq!(&buf[..n], b"e-12");
    let n = exponent(&mut buf, 0, b'p');
    assert_eq!(&buf[..n], b"p+0");
    let n = exponent(&mut buf, -3, b'P');
    assert_eq!(&buf[..n], b"P-3");
    let n = exponent(&mut buf, 123456, b'e');
    assert_eq!(&buf[..n], b"e+123456");
}

#[test]
fn helpers_match_the_donor() {
    assert!(is_digit(u32::from(b'0')));
    assert!(is_digit(u32::from(b'9')));
    assert!(!is_digit(u32::from(b'a')));
    assert_eq!(to_digit(b'7'), 7);
    assert_eq!(to_char(7), b'7');
    let mut buf = [0u8; 64];
    let a = ultoa(1234, &mut buf, 10, false, false);
    let b = ujtoa(1234, &mut buf, 10, false, false);
    assert_eq!(a, b);
}
