// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the wire ABI.

use super::*;

#[test]
fn wire_sizes_match_the_kernel() {
    assert_eq!(core::mem::size_of::<Record>(), 64);
    assert_eq!(core::mem::size_of::<ProbeInfo>(), 52);
    assert_eq!(core::mem::size_of::<CpuStats>(), 32);
}

#[test]
fn syscall_numbers_are_the_s57_values() {
    assert_eq!(PROBE_LIST, 0x90);
    assert_eq!(ENABLE, 0x91);
    assert_eq!(DISABLE, 0x92);
    assert_eq!(READ, 0x93);
    assert_eq!(STATS, 0x94);
    assert_eq!(CLEAR, 0x95);
}

#[test]
fn probe_strings_are_nul_trimmed() {
    let mut info = ProbeInfo::default();
    info.provider[..7].copy_from_slice(b"syscall");
    info.name[..5].copy_from_slice(b"entry");
    assert_eq!(provider(&info), "syscall");
    assert_eq!(name(&info), "entry");
}

#[test]
fn an_empty_probe_field_is_an_empty_string() {
    let info = ProbeInfo::default();
    assert_eq!(provider(&info), "");
    assert_eq!(name(&info), "");
}
