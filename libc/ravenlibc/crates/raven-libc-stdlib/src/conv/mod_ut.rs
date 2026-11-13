// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::assertions_on_constants
)]

//! The group surface: `Scan`, the alias family and the wide entry point.

use super::{IntStatus, Scan, scan, scan_integer, strtol, strtoul};

#[test]
fn scan_shape() {
    let s: Scan = scan_integer(b"-0x10", 0, true);
    assert_eq!(
        s,
        Scan {
            value: 16,
            negative: true,
            consumed: 5,
            status: IntStatus::Ok
        }
    );
}

#[test]
fn generic_wide_entry_point() {
    let units: &[u32] = &['3' as u32, '7' as u32];
    let s = scan(units, 10, true);
    assert_eq!(s.value, 37);
    assert_eq!(s.consumed, 2);
}

#[test]
fn reexported_narrow_functions() {
    assert_eq!(strtol(b"10", 2).0, 2);
    assert_eq!(strtoul(b"10", 2).0, 2);
}

#[test]
fn status_equality_and_copy() {
    let a = IntStatus::Ok;
    let b = a;
    assert_eq!(a, b);
    assert_ne!(IntStatus::Ok, IntStatus::NoDigits);
}

#[test]
fn all_alias_functions() {
    assert_eq!(super::strtoll(b"10", 10), super::strtol(b"10", 10));
    assert_eq!(super::strtoimax(b"10", 10), super::strtol(b"10", 10));
    assert_eq!(super::strtoull(b"10", 10), super::strtoul(b"10", 10));
    assert_eq!(super::strtoumax(b"10", 10), super::strtoul(b"10", 10));
}

#[test]
fn scan_struct_is_copy() {
    let scan = scan_integer(b"1", 10, true);
    let copied = scan;
    assert_eq!(scan, copied);
}

#[test]
fn statuses_are_distinct() {
    assert_ne!(IntStatus::Ok, IntStatus::Range);
    assert_ne!(IntStatus::Range, IntStatus::InvalidBase);
    assert_ne!(IntStatus::InvalidBase, IntStatus::NoDigits);
}

#[test]
fn code_units_convert() {
    assert_eq!(crate::CodeUnit::to_u32(7u8), 7);
    assert_eq!(crate::CodeUnit::to_u32(0x1f600u32), 0x1f600);
}
