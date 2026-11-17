// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the public segment view.

use super::*;

#[test]
fn accessors_describe_the_extent() {
    let segment = Segment::new(0x1000, 0x120, SegmentState::Allocated);
    assert_eq!(segment.base(), 0x1000);
    assert_eq!(segment.size(), 0x120);
    assert_eq!(segment.end(), 0x1120);
    assert_eq!(segment.state(), SegmentState::Allocated);
    assert!(!segment.is_free());
}

#[test]
fn free_state_is_reported() {
    let segment = Segment::new(0x2000, 0x10, SegmentState::Free);
    assert!(segment.is_free());
    assert!(SegmentState::Free.is_free());
    assert!(!SegmentState::Allocated.is_free());
}
