// SPDX-License-Identifier: LicenseRef-PublicDomain
// Written by J.T. Conklin <jtc@netbsd.org>.
// Public domain.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/l64a.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: l64a.c,v 1.5 2005/08/08 08:05:36 espie Exp $)

#![allow(clippy::arithmetic_side_effects)] // RavenBSD: 6-bit shift arithmetic

//! `l64a`: the inverse of `a64l` into an eight-byte buffer.
//!
//! A negative `value` is the shell's `EINVAL`/`NULL` path (the donor checks the sign before
//! touching its static buffer); this function writes an empty string in that case.

/// Encode `value` (0..=2^36-1 usefully) into `out`, NUL-terminated.
pub fn l64a(value: i64, out: &mut [u8; 8]) {
    let mut cursor = 0usize;
    if value < 0 {
        if let Some(slot) = out.first_mut() {
            *slot = 0;
        }
        return;
    }
    let mut value = value as u64;
    let mut index = 0usize;
    while value != 0 && index < 6 {
        let digit = (value & 0x3f) as u8;
        let byte = if digit < 2 {
            digit + b'.'
        } else if digit < 12 {
            digit.wrapping_add(b'0').wrapping_sub(2)
        } else if digit < 38 {
            digit.wrapping_add(b'A').wrapping_sub(12)
        } else {
            digit.wrapping_add(b'a').wrapping_sub(38)
        };
        if let Some(slot) = out.get_mut(index) {
            *slot = byte;
        }
        cursor = index.wrapping_add(1);
        value >>= 6;
        index = index.wrapping_add(1);
    }
    if let Some(slot) = out.get_mut(cursor) {
        *slot = 0;
    }
}

#[cfg(test)]
#[path = "l64a_ut.rs"]
mod tests;
