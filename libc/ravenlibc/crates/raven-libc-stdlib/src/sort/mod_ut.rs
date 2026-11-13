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

//! The sort group surface and error equality.

use super::{SortError, bsearch, heapsort, mergesort, qsort};

#[test]
fn error_variants_are_distinct() {
    assert_ne!(SortError::Inval, SortError::NoMem);
    assert_eq!(SortError::NoMem, SortError::NoMem);
}

#[test]
fn bsd_extensions_accept_empty_input() {
    let mut empty: [u8; 0] = [];
    assert_eq!(heapsort(&mut empty, 8, |_, _| 0), Ok(()));
    assert_eq!(mergesort(&mut empty, 8, |_, _| 0), Ok(()));
}

#[test]
fn bsearch_is_reexported() {
    let key = 1u32.to_le_bytes();
    let base = 1u32.to_le_bytes();
    assert_eq!(
        bsearch(&base, 4, |element| {
            if element == key.as_slice() { 0 } else { 1 }
        }),
        Some(0)
    );
}

#[test]
fn heapsort_single_element() {
    let mut one = 7u32.to_le_bytes();
    assert_eq!(heapsort(&mut one, 4, |_, _| 0), Ok(()));
}

#[test]
fn mergesort_single_element() {
    let mut one = 7u64.to_le_bytes();
    assert_eq!(mergesort(&mut one, 8, |_, _| 0), Ok(()));
}

#[test]
fn bsearch_single_match() {
    let base = 5u32.to_le_bytes();
    let key = 5u32.to_le_bytes();
    assert_eq!(
        bsearch(&base, 4, |element| {
            if element == key.as_slice() { 0 } else { 1 }
        }),
        Some(0)
    );
}

#[test]
fn bsearch_single_miss() {
    let base = 5u32.to_le_bytes();
    let key = 6u32.to_le_bytes();
    assert_eq!(
        bsearch(&base, 4, |element| {
            if element == key.as_slice() { 0 } else { -1 }
        }),
        None
    );
}

#[test]
fn qsort_empty_is_a_noop() {
    let mut empty: [u8; 0] = [];
    qsort(&mut empty, 4, |_, _| 0);
    assert!(empty.is_empty());
}
