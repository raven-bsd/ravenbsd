// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for address windows.

use super::*;

#[test]
fn rejects_empty_and_overflowing_windows() {
    assert!(Region::new(0, 0).is_none());
    assert!(Region::new(u64::MAX, 1).is_none());
    assert!(Region::new(u64::MAX, u64::MAX).is_none());
    assert!(Region::new(0x1f00_0000, 0x41_0000).is_some());
}

#[test]
fn contains_is_half_open() {
    let region = Region::new(0x1000, 0x100).expect("region");
    assert!(!region.contains(0x0FFF));
    assert!(region.contains(0x1000));
    assert!(region.contains(0x10FF));
    assert!(!region.contains(0x1100));
}

#[test]
fn contains_region_requires_full_containment() {
    let window = Region::new(0x1f00_0000, 0x41_0000).expect("window");
    let inside = Region::new(0x1f00_8000, 0x4000).expect("inside");
    let overlapping = Region::new(0x1f00_0000, 0x42_0000).expect("overlap");
    assert!(window.contains_region(inside));
    assert!(!window.contains_region(overlapping));
    assert!(window.contains_region(window));
}

#[test]
fn offset_of_rejects_outside_addresses() {
    let region = Region::new(0x1f00_0000, 0x41_0000).expect("region");
    assert_eq!(region.offset_of(0x1f00_0000), Some(0));
    assert_eq!(region.offset_of(0x1f00_3000), Some(0x3000));
    assert_eq!(region.offset_of(0x1eff_ffff), None);
    assert_eq!(region.offset_of(0x1f41_0000), None);
}
