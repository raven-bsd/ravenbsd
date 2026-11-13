// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! Wide case comparison tests.

use super::*;

#[test]
fn ascii_case_folds() {
    assert_eq!(wcscasecmp(&[b'A' as u32], &[b'a' as u32]), 0);
    assert!(wcscasecmp(&[b'A' as u32], &[b'b' as u32]) < 0);
    assert_eq!(
        wcsncasecmp(&[b'A' as u32, b'B' as u32], &[b'a' as u32, b'b' as u32], 2),
        0
    );
}

#[test]
fn unicode_simple_folding() {
    assert_eq!(wcscasecmp(&[0xc9], &[0xe9]), 0);
    assert_ne!(wcscasecmp(&[0xdf], &[0x1e9e]), 0);
}

#[test]
fn zero_terminates_the_comparison() {
    assert_eq!(wcscasecmp(&[b'A' as u32, 0], &[b'a' as u32]), 0);
    assert_eq!(wcsncasecmp(&[b'A' as u32], &[b'a' as u32], 0), 0);
}
