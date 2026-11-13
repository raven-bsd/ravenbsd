// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::*;

#[test]
fn version_and_budget() {
    assert_eq!(UNICODE_VERSION, (15, 1, 0));
    const {
        assert!(TABLE_BYTES <= 48 * 1024);
    }
}

#[test]
fn ascii_properties_come_from_the_table() {
    assert!(is_alpha(u32::from(b'A')));
    assert!(!is_alpha(u32::from(b'0')));
    assert!(is_punct(u32::from(b'!')));
    assert!(!is_punct(u32::from(b'A')));
    assert_eq!(width_bits(u32::from(b'A')), 1);
    assert_eq!(simple_upper(u32::from(b'a')), u32::from(b'A'));
    assert_eq!(simple_lower(u32::from(b'A')), u32::from(b'a'));
}

#[test]
fn out_of_range_is_safe() {
    assert!(!is_alpha(0x110000));
    assert!(!is_punct(u32::MAX));
    assert_eq!(width_bits(u32::MAX), 1);
    assert_eq!(simple_upper(u32::MAX), u32::MAX);
    assert_eq!(simple_lower(u32::MAX), u32::MAX);
}
