// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the bounded result copies.

use super::*;
use core::ffi::c_char;

#[test]
fn copies_and_terminates() {
    let mut buffer = [0x7fu8; 8];
    // SAFETY: the array has eight writable bytes.
    let copied = unsafe { write_cstr(buffer.as_mut_ptr().cast::<c_char>(), b"abc", 8) };
    assert_eq!(copied, 3);
    assert_eq!(&buffer[..4], b"abc\0");
    assert_eq!(buffer[4], 0x7f);
}

#[test]
fn truncates_inside_cap() {
    let mut buffer = [0x7fu8; 4];
    // SAFETY: the array has four writable bytes.
    let copied = unsafe { write_cstr(buffer.as_mut_ptr().cast::<c_char>(), b"abcdef", 4) };
    assert_eq!(copied, 3);
    assert_eq!(&buffer[..4], b"abc\0");
}

#[test]
fn a_zero_cap_writes_nothing() {
    let mut buffer = [0x7fu8; 2];
    // SAFETY: cap zero writes nothing.
    let copied = unsafe { write_cstr(buffer.as_mut_ptr().cast::<c_char>(), b"a", 0) };
    assert_eq!(copied, 0);
    assert_eq!(buffer, [0x7f, 0x7f]);
}

#[test]
fn a_null_destination_is_a_no_op() {
    // SAFETY: a NULL destination writes nothing.
    assert_eq!(unsafe { write_bytes(core::ptr::null_mut(), b"a", 1) }, 0);
}

#[test]
fn raw_bytes_copy_without_a_terminator() {
    let mut buffer = [0u8; 3];
    // SAFETY: the array has three writable bytes.
    let copied = unsafe { write_bytes(buffer.as_mut_ptr(), b"xy", 3) };
    assert_eq!(copied, 2);
    assert_eq!(buffer, *b"xy\0");
}
