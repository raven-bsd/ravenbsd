// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the per-CPU GDT/TSS layout.

use super::*;

#[test]
fn star_selectors_match_sysret_arithmetic() {
    let star = STAR_USER_BASE as u64;
    assert_eq!(star.saturating_add(8) & 0xffff, USER_SS as u64);
    assert_eq!(star.saturating_add(16) & 0xffff, USER_CS as u64);
    assert_eq!(KERNEL_CS, 0x08);
    assert_eq!(KERNEL_SS, 0x10);
}

#[test]
fn every_cpu_gets_the_fixed_selectors_and_a_tss_descriptor() {
    let table = build_gdt(0xFFFF_8000_1234_0000);
    assert_eq!(table[0], 0);
    assert_eq!(table[1], KERNEL_CODE_ENTRY);
    assert_eq!(table[2], KERNEL_DATA_ENTRY);
    assert_eq!(table[3], USER_DATA_ENTRY);
    assert_eq!(table[4], USER_CODE_ENTRY);
    // TSS descriptor is present and has the 64-bit available TSS access byte 0x89.
    assert_eq!((table[5] >> 40) & 0xff, 0x89);
}
