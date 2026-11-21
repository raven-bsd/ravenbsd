// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the x86-64 paging constants.

use super::{entry_address, entry_flags, is_canonical};

#[test]
fn canonical_low_half() {
    assert!(is_canonical(0));
    assert!(is_canonical(0x0000_7FFF_FFFF_FFFF));
}

#[test]
fn canonical_high_half() {
    assert!(is_canonical(0xFFFF_8000_0000_0000));
    assert!(is_canonical(0xFFFF_FFFF_FFFF_FFFF));
}

#[test]
fn canonical_hole_is_rejected() {
    assert!(!is_canonical(0x0000_8000_0000_0000));
    assert!(!is_canonical(0x0000_FFFF_FFFF_FFFF));
    assert!(!is_canonical(0xFFFF_0000_0000_0000));
    assert!(!is_canonical(0xFFFF_7FFF_FFFF_FFFF));
}

#[test]
fn entry_address_and_flags_are_disjoint() {
    let entry = 0x1234_5067u64;
    assert_eq!(entry_address(entry), 0x1234_5000);
    assert_eq!(entry_flags(entry), 0x067);
}
