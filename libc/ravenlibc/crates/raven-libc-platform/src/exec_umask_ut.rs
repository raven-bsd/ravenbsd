// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the exec umask hand-off.

use super::{ENV_NAME, decode, encode};

#[test]
fn env_name_is_private() {
    assert_eq!(ENV_NAME, b"__RAVEN_UMASK");
}

#[test]
fn encodes_four_octal_digits() {
    assert_eq!(encode(0o027), b"__RAVEN_UMASK=0027".to_vec());
    assert_eq!(encode(0o077), b"__RAVEN_UMASK=0077".to_vec());
    assert_eq!(encode(0o022), b"__RAVEN_UMASK=0022".to_vec());
    assert_eq!(encode(0), b"__RAVEN_UMASK=0000".to_vec());
}

#[test]
fn masks_the_high_bits() {
    assert_eq!(encode(0o7027), b"__RAVEN_UMASK=0027".to_vec());
}

#[test]
fn decodes_what_it_encodes() {
    for mask in [0o000, 0o022, 0o027, 0o077, 0o777, 0o640, 0o755] {
        let encoded = encode(mask);
        let value = encoded.split(|byte| *byte == b'=').nth(1).unwrap();
        assert_eq!(decode(value), Some(mask));
    }
}

#[test]
fn rejects_non_octal_or_oversized_values() {
    assert_eq!(decode(b""), None);
    assert_eq!(decode(b"8"), None);
    assert_eq!(decode(b"02x7"), None);
    assert_eq!(decode(b"00270"), None);
    assert_eq!(decode(b"  27"), None);
}

#[test]
fn accepts_one_to_three_digits() {
    assert_eq!(decode(b"7"), Some(0o007));
    assert_eq!(decode(b"27"), Some(0o027));
    assert_eq!(decode(b"027"), Some(0o027));
}
