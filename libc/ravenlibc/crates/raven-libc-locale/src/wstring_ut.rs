// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]

//! Wide-string tests.

use super::*;

#[test]
fn length_and_comparison() {
    assert_eq!(wcslen(&[1, 2, 3]), 3);
    assert_eq!(wcslen(&[1, 0, 3]), 1);
    assert_eq!(wcscmp(&[1, 2], &[1, 2]), 0);
    assert!(wcscmp(&[1, 2], &[1, 3]) < 0);
    assert!(wcscmp(&[1], &[1, 2]) < 0);
    assert_eq!(wcsncmp(&[1, 2], &[1, 3], 1), 0);
}

#[test]
fn search_helpers() {
    assert_eq!(wcschr(&[1, 2, 3], 2), Some(1));
    assert_eq!(wcschr(&[1, 2, 3], 9), None);
    assert_eq!(wcsrchr(&[1, 2, 1], 1), Some(2));
    assert_eq!(wcscspn(&[1, 2, 3], &[2]), 1);
    assert_eq!(wcsspn(&[1, 2, 3], &[1, 2]), 2);
    assert_eq!(wcspbrk(&[1, 2, 3], &[3]), Some(2));
    assert_eq!(wcsstr(&[1, 2, 3, 2, 3], &[2, 3]), Some(1));
    assert_eq!(wcsstr(&[1], &[1, 2]), None);
    assert_eq!(wcsstr(&[1], &[]), Some(0));
}

#[test]
fn copy_and_append() {
    let mut buffer = [0u32; 5];
    assert_eq!(wcscpy(&mut buffer, &[1, 2, 3]), 3);
    assert_eq!(&buffer[..3], &[1, 2, 3]);
    assert_eq!(wcscat(&mut buffer, &[4]), 4);
    assert_eq!(&buffer[..4], &[1, 2, 3, 4]);
    let mut buffer = [0u32; 4];
    assert_eq!(wcsncpy(&mut buffer, &[1, 2, 3, 4], 2), 2);
    assert_eq!(&buffer[..2], &[1, 2]);
    assert_eq!(wcsncat(&mut buffer, &[3, 4], 1), 3);
    assert_eq!(&buffer[..3], &[1, 2, 3]);
}

#[test]
fn openbsd_bounded_functions() {
    let mut buffer = [0u32; 4];
    assert_eq!(wcslcpy(&mut buffer, &[1, 2, 3, 4]), 4);
    assert_eq!(&buffer[..], &[1, 2, 3, 0]);
    let mut buffer = [1u32, 2, 0, 0, 0];
    assert_eq!(wcslcat(&mut buffer, &[3, 4, 5]), 5);
    assert_eq!(&buffer[..], &[1, 2, 3, 4, 0]);
}

#[test]
fn memory_helpers() {
    let mut buffer = [0u32; 4];
    assert_eq!(wmemcpy(&mut buffer, &[1, 2]), 2);
    assert_eq!(&buffer[..2], &[1, 2]);
    assert_eq!(wmemchr(&[1, 2, 3], 2), Some(1));
    assert_eq!(wmemcmp(&[1, 2], &[1, 2], 2), 0);
    assert!(wmemcmp(&[0x8000_0000], &[1], 1) > 0);
    assert_eq!(wmemset(&mut buffer, 7, 2), 2);
    assert_eq!(&buffer[..2], &[7, 7]);
    assert_eq!(wmemmove(&mut buffer, &[8, 9]), 2);
    assert_eq!(&buffer[..2], &[8, 9]);
}

#[test]
fn tokeniser() {
    let mut cursor: &[u32] = &[];
    let token = wcstok(&[1, 2, 0, 3, 2, 4], &[2], &mut cursor);
    assert_eq!(token, Some(&[1][..]));
    let token = wcstok(&[], &[2], &mut cursor);
    assert_eq!(token, Some(&[0, 3][..]));
}
