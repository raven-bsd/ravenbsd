// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `MemView` bounds behaviour.

use super::*;

#[test]
fn a_read_inside_one_range_returns_the_bytes() {
    let bytes = [1u8, 2, 3, 4];
    let view = SliceMem::new(0x1000, &bytes);
    assert_eq!(view.read(0x1000, 4), Some(&bytes[..]));
    assert_eq!(view.read(0x1001, 2), Some(&bytes[1..3]));
    assert_eq!(view.read(0x1004, 0), Some(&b""[..]));
}

#[test]
fn a_read_starting_before_the_range_is_none() {
    let bytes = [1u8, 2, 3, 4];
    let view = SliceMem::new(0x1000, &bytes);
    assert_eq!(view.read(0x0fff, 1), None);
    assert_eq!(view.read(0x0fff, 4), None);
}

#[test]
fn a_read_ending_past_the_range_is_none() {
    let bytes = [1u8, 2, 3, 4];
    let view = SliceMem::new(0x1000, &bytes);
    assert_eq!(view.read(0x1002, 3), None);
    assert_eq!(view.read(0x1004, 1), None);
}

#[test]
fn a_zero_length_read_at_the_end_is_allowed() {
    let bytes = [1u8, 2, 3, 4];
    let view = SliceMem::new(0x1000, &bytes);
    assert_eq!(view.read(0x1004, 0), Some(&b""[..]));
    assert_eq!(view.read(0x1005, 0), None);
}

#[test]
fn an_address_overflow_is_none() {
    let bytes = [1u8, 2, 3, 4];
    let view = SliceMem::new(0x1000, &bytes);
    assert_eq!(view.read(u64::MAX, 1), None);
    assert_eq!(view.read(u64::MAX, 0), None);
    assert_eq!(view.read(u64::MAX - 1, 2), None);
}

#[test]
fn a_cstr_stops_at_the_nul() {
    let bytes = b"libc.so.1\0next";
    let view = SliceMem::new(0x2000, bytes);
    assert_eq!(view.cstr(0x2000), Some(&b"libc.so.1"[..]));
    assert_eq!(view.cstr(0x2009), Some(&b""[..]));
    // "next" has no NUL inside the range, so the string is unterminated.
    assert_eq!(view.cstr(0x200a), None);
}

#[test]
fn a_cstr_without_a_nul_in_the_range_is_none() {
    let bytes = b"libc.so.1";
    let view = SliceMem::new(0x2000, bytes);
    assert_eq!(view.cstr(0x2000), None);
    assert_eq!(view.cstr(0x200a), None);
    assert_eq!(view.cstr(0x200b), None);
}

#[test]
fn a_cstr_looking_across_a_range_end_is_none() {
    let left = SliceMem::new(0, b"ab");
    let right = SliceMem::new(3, b"\0cd");
    let mut set = MemSet::new();
    set.push(left);
    set.push(right);
    assert_eq!(set.cstr(0), None);
    assert_eq!(set.cstr(3), Some(&b""[..]));
}

#[test]
fn memset_reads_from_the_containing_range() {
    let left = SliceMem::new(0x1000, b"abcd");
    let right = SliceMem::new(0x2000, b"efgh");
    let mut set = MemSet::new();
    set.push(left);
    set.push(right);
    assert_eq!(set.read(0x1002, 2), Some(&b"cd"[..]));
    assert_eq!(set.read(0x2001, 3), Some(&b"fgh"[..]));
    assert_eq!(set.read(0x1800, 1), None);
}

#[test]
fn a_memset_read_straddling_two_ranges_is_none() {
    let left = SliceMem::new(0x1000, b"abcd");
    let right = SliceMem::new(0x1004, b"efgh");
    let mut set = MemSet::new();
    set.push(left);
    set.push(right);
    // Adjacent ranges still do not merge.
    assert_eq!(set.read(0x1002, 4), None);
    assert_eq!(set.read(0x1003, 1), Some(&b"d"[..]));
    assert_eq!(set.read(0x1004, 1), Some(&b"e"[..]));
}

#[test]
fn an_empty_view_reads_nothing() {
    let set = MemSet::new();
    assert_eq!(set.read(0, 0), None);
    assert_eq!(set.read(0, 1), None);
    assert_eq!(set.cstr(0), None);
}

#[test]
fn a_large_zero_length_read_at_the_end_is_bound_checked() {
    let bytes = [0u8; 8];
    let view = SliceMem::new(0xffff_ffff_ffff_fff0, &bytes);
    assert_eq!(view.read(0xffff_ffff_ffff_fff0, 8), Some(&bytes[..]));
    assert_eq!(view.read(0xffff_ffff_ffff_fff0, 9), None);
    // A range whose end would wrap past the address space cannot be read.
    let wrapped = SliceMem::new(0xffff_ffff_ffff_fff8, &bytes);
    assert_eq!(wrapped.read(0xffff_ffff_ffff_fff8, 8), None);
}
