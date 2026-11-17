// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the C string helpers.

use super::*;
use core::ffi::c_char;

fn c(text: &[u8]) -> *const c_char {
    text.as_ptr().cast::<c_char>()
}

#[test]
fn a_null_pointer_is_the_empty_slice() {
    // SAFETY: a NULL pointer is explicitly allowed by the helper contract.
    assert!(unsafe { slice(core::ptr::null()) }.is_empty());
}

#[test]
fn reads_a_one_byte_string() {
    let text = b"x\0";
    // SAFETY: `text` is NUL terminated.
    assert_eq!(unsafe { slice(c(text)) }, b"x");
}

#[test]
fn reads_a_four_kibibyte_string() {
    let mut text = vec![b'a'; 4096];
    text.push(0);
    // SAFETY: `text` is NUL terminated.
    assert_eq!(unsafe { slice(c(&text)) }.len(), 4096);
}

#[test]
fn slice_n_stops_at_the_limit() {
    let text = b"abcdef\0";
    // SAFETY: five readable bytes.
    assert_eq!(unsafe { slice_n(c(text), 5) }, b"abcde");
}

#[test]
fn slice_n_stops_at_the_nul() {
    let text = b"abc\0xyz";
    // SAFETY: the NUL is before `n`.
    assert_eq!(unsafe { slice_n(c(text), 7) }, b"abc");
}

#[test]
fn a_null_with_zero_length_is_empty_not_a_dangling_slice() {
    // SAFETY: a zero length with a NULL pointer is the documented empty case.
    assert!(unsafe { slice_mut(core::ptr::null_mut(), 0) }.is_empty());
}

#[test]
fn slice_mut_exposes_the_buffer() {
    let mut buffer = [0u8; 3];
    // SAFETY: the array has three writable bytes.
    let view = unsafe { slice_mut(buffer.as_mut_ptr().cast::<c_char>(), 3) };
    view[0] = 7;
    assert_eq!(buffer[0], 7);
}
