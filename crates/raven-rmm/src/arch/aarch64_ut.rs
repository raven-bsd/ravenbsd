// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the aarch64 translation-table constants.

use super::{is_canonical, is_kernel_half, is_user_half};

#[test]
fn canonical_halves() {
    assert!(is_canonical(0));
    assert!(is_canonical(0x0000_7FFF_FFFF_FFFF));
    // The full 48-bit lower half belongs to TTBR0; there is no x86-64-style hole.
    assert!(is_canonical(0x0000_8000_0000_0000));
    assert!(is_canonical(0x0000_FFFF_FFFF_FFFF));
    assert!(is_canonical(0xFFFF_0000_0000_0000));
    assert!(is_canonical(0xFFFF_8000_0000_0000));
    assert!(is_canonical(0xFFFF_FFFF_FFFF_FFFF));
}

#[test]
fn non_canonical_is_rejected() {
    // With T0SZ = T1SZ = 16 the whole 48-bit lower half is valid TTBR0 space (there is no
    // x86-64-style hole), so only a partially-set top byte is non-canonical.
    assert!(!is_canonical(0x8000_0000_0000_0000));
    assert!(!is_canonical(0x7FFF_0000_0000_0000));
}

#[test]
fn half_classification() {
    assert!(is_user_half(0x0000_0000_1000_0000));
    assert!(is_kernel_half(0xFFFF_FFFF_8020_0000));
    assert!(is_kernel_half(0xFFFF_C000_0000_0000));
}
