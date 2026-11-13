// SPDX-License-Identifier: LicenseRef-PublicDomain
// Written by J.T. Conklin <jtc@netbsd.org>.
// Public domain.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/a64l.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: a64l.c,v 1.5 2005/08/08 08:05:36 espie Exp $)

#![allow(clippy::arithmetic_side_effects)] // RavenBSD: 6-bit shift arithmetic

//! `a64l`: up to six little-endian base-64 digits.

use raven_libc_abi::errno::{EINVAL, Errno};

/// Decode `s`; the LP64 result is the donor's positive 36-bit `long` (no sign extension).
///
/// # Errors
///
/// `EINVAL` for a byte outside the 64-character a64l alphabet.
pub fn a64l(s: &[u8]) -> Result<i64, Errno> {
    let mut value = 0u64;
    let mut shift = 0u32;
    let mut index = 0usize;
    while index < 6 {
        let Some(&byte) = s.get(index) else {
            break;
        };
        if byte == 0 {
            break;
        }
        let digit = if (b'.'..=b'/').contains(&byte) {
            u64::from(byte - b'.')
        } else if byte.is_ascii_digit() {
            u64::from(byte - b'0') + 2
        } else if byte.is_ascii_uppercase() {
            u64::from(byte - b'A') + 12
        } else if byte.is_ascii_lowercase() {
            u64::from(byte - b'a') + 38
        } else {
            return Err(Errno(EINVAL as u16));
        };
        value |= digit << shift;
        shift = shift.wrapping_add(6);
        index = index.wrapping_add(1);
    }
    Ok(value as i64)
}

#[cfg(test)]
#[path = "a64l_ut.rs"]
mod tests;
