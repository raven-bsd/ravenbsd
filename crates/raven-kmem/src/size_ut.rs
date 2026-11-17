// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the size-class selection.

use super::*;

#[test]
fn classes_are_sorted_and_in_range() {
    for pair in SIZE_CLASSES.windows(2) {
        assert!(pair[0] < pair[1], "classes must ascend");
    }
    assert_eq!(*SIZE_CLASSES.last().expect("classes"), LARGE_THRESHOLD);
    for &class in &SIZE_CLASSES {
        assert!(class >= 8);
        assert!(class % 8 == 0, "every class is a multiple of 8");
        assert!(class_align(class).is_power_of_two());
        assert!(class_align(class) <= class);
    }
}

#[test]
fn class_for_finds_the_smallest_fit() {
    assert_eq!(class_for(1), Some(0));
    assert_eq!(class_for(8), Some(0));
    assert_eq!(class_for(9), Some(1));
    assert_eq!(class_for(16), Some(1));
    assert_eq!(class_for(17), Some(2));
    assert_eq!(class_for(24), Some(2));
    assert_eq!(class_for(48), Some(4));
    assert_eq!(class_for(8192), Some(18));
    assert_eq!(class_for(8193), None);
    assert_eq!(class_for(usize::MAX), None);
}

#[test]
fn class_align_matches_the_spec() {
    assert_eq!(class_align(8), 8);
    assert_eq!(class_align(16), 16);
    assert_eq!(class_align(24), 16);
    assert_eq!(class_align(8192), 16);
}

#[test]
fn classify_routes_class_versus_large() {
    // Ordinary requests land in a class.
    assert_eq!(classify(1, 8), Some(0));
    assert_eq!(classify(8, 8), Some(0));
    assert_eq!(classify(16, 16), Some(1));
    assert_eq!(classify(24, 16), Some(2));
    assert_eq!(classify(100, 16), Some(7));
    assert_eq!(classify(8192, 16), Some(18));
    // A request whose alignment exceeds its class goes large.
    assert_eq!(classify(8, 16), None);
    // Over-large or over-aligned requests go large.
    assert_eq!(classify(8193, 8), None);
    assert_eq!(classify(100, 32), None);
    assert_eq!(classify(64, 64), None);
}

#[test]
fn classify_is_consistent_for_alloc_free_pairs() {
    // The pure function must agree for any size/align an allocator passes back and forth.
    for size in [1usize, 7, 8, 9, 16, 17, 24, 100, 1024, 8192, 8193] {
        for align in [1usize, 2, 4, 8, 16, 32, 4096] {
            let a = classify(size, align);
            let b = classify(size, align);
            assert_eq!(a, b);
        }
    }
}
