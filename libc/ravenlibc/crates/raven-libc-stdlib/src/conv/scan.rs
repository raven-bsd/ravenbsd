// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Shared integer scanner behind `strtol`, `strtoul`, `atoi`, the wide `wcstol` family and
//! `scanf` (RD-23, O-5).
//!
//! The scanner is the OpenBSD cutoff algorithm of `openbsd:lib/libc/stdlib/strtol.c:82-140`,
//! rewritten over a [`CodeUnit`] slice with checked arithmetic. The donor's locale classes
//! become ASCII-only tests (T-4) and the C23 `0b`/`0B` prefix is added for base 0 and 2
//! (A-stdlib-6, RD-26). The C original reads one byte past the terminating NUL on an empty
//! string (its `do { c = *s++ } while (isspace(c))` loop); a slice cannot, so the scanner stops
//! at the end instead (`// RavenBSD: slice index replaces pointer`).

use crate::CodeUnit;
use crate::IntStatus;
use crate::Scan;

/// Wrap `u32` as the ASCII digit value, or `None` for a non-digit.
fn digit_value(c: u32) -> Option<u32> {
    if c >= u32::from(b'0') && c <= u32::from(b'9') {
        return Some(c.wrapping_sub(u32::from(b'0')));
    }
    if c >= u32::from(b'A') && c <= u32::from(b'Z') {
        return Some(c.wrapping_sub(u32::from(b'A')).wrapping_add(10));
    }
    if c >= u32::from(b'a') && c <= u32::from(b'z') {
        return Some(c.wrapping_sub(u32::from(b'a')).wrapping_add(10));
    }
    None
}

/// C `isspace` for the byte-transparent C locale: exactly `' ' \t \n \v \f \r` (T-4).
fn is_space(c: u32) -> bool {
    c == u32::from(b' ')
        || c == u32::from(b'\t')
        || c == u32::from(b'\n')
        || c == 0x0b
        || c == 0x0c
        || c == u32::from(b'\r')
}

fn is_hex(c: u32) -> bool {
    (c >= u32::from(b'0') && c <= u32::from(b'9'))
        || (c >= u32::from(b'a') && c <= u32::from(b'f'))
        || (c >= u32::from(b'A') && c <= u32::from(b'F'))
}

fn is_binary(c: u32) -> bool {
    c == u32::from(b'0') || c == u32::from(b'1')
}

/// Scan an integer in `base` from `s`, which excludes the terminating NUL.
///
/// `signed` selects the `strtol` range (`i64`) rather than the `strtoul` range (`u64`); the
/// magnitude is returned in [`Scan::value`] with [`Scan::negative`] recording the sign, exactly
/// as the shared narrow/wide scanner does. `base` is validated here: `< 0`, `1` and `> 36` give
/// [`IntStatus::InvalidBase`] with `consumed == 0`.
#[must_use]
pub fn scan_integer<C: CodeUnit>(s: &[C], base: i32, signed: bool) -> Scan {
    let mut scan = Scan {
        value: 0,
        negative: false,
        consumed: 0,
        status: IntStatus::Ok,
    };

    // RavenBSD: invalid base first, before whitespace, as strtol.c:56-59 does.
    if base < 0 || base == 1 || base > 36 {
        scan.status = IntStatus::InvalidBase;
        return scan;
    }

    let mut index = 0usize;
    while let Some(&unit) = s.get(index) {
        if !is_space(unit.to_u32()) {
            break;
        }
        index = index.wrapping_add(1);
    }

    let mut negative = false;
    if let Some(&unit) = s.get(index) {
        let c = unit.to_u32();
        if c == u32::from(b'-') {
            negative = true;
            index = index.wrapping_add(1);
        } else if c == u32::from(b'+') {
            index = index.wrapping_add(1);
        }
    }
    scan.negative = negative;

    let mut base = base;
    let mut start = index;

    if base == 0 || base == 16 {
        let zero = s.get(start).map(|unit| unit.to_u32()) == Some(u32::from(b'0'));
        let marker = match s.get(start.wrapping_add(1)) {
            Some(&unit) => unit.to_u32(),
            None => 0,
        };
        let following = match s.get(start.wrapping_add(2)) {
            Some(&unit) => unit.to_u32(),
            None => 0,
        };
        if zero && (marker == u32::from(b'x') || marker == u32::from(b'X')) && is_hex(following) {
            start = start.wrapping_add(2);
            base = 16;
        }
    }
    // RavenBSD: C23 binary prefix, absent from the BSD donor (A-stdlib-6, RD-26).
    if base == 0 || base == 2 {
        let zero = s.get(start).map(|unit| unit.to_u32()) == Some(u32::from(b'0'));
        let marker = match s.get(start.wrapping_add(1)) {
            Some(&unit) => unit.to_u32(),
            None => 0,
        };
        let following = match s.get(start.wrapping_add(2)) {
            Some(&unit) => unit.to_u32(),
            None => 0,
        };
        if zero && (marker == u32::from(b'b') || marker == u32::from(b'B')) && is_binary(following)
        {
            start = start.wrapping_add(2);
            base = 2;
        }
    }
    if base == 0 {
        let zero = s.get(start).map(|unit| unit.to_u32()) == Some(u32::from(b'0'));
        base = if zero { 8 } else { 10 };
    }

    let radix = u64::from(base as u32);
    let limit = if signed {
        if negative {
            1u64 << 63
        } else {
            (1u64 << 63) - 1
        }
    } else {
        u64::MAX
    };

    let mut acc = 0u64;
    let mut any = false;
    let mut overflow = false;
    index = start;
    while let Some(&unit) = s.get(index) {
        let Some(digit) = digit_value(unit.to_u32()) else {
            break;
        };
        if u64::from(digit) >= radix {
            break;
        }
        any = true;
        if !overflow {
            let next = acc
                .checked_mul(radix)
                .and_then(|value| value.checked_add(u64::from(digit)));
            match next {
                Some(value) if value <= limit => acc = value,
                _ => overflow = true,
            }
        }
        index = index.wrapping_add(1);
    }

    if !any {
        scan.status = IntStatus::NoDigits;
        scan.consumed = 0;
        scan.value = 0;
        return scan;
    }
    if overflow {
        scan.status = IntStatus::Range;
        scan.value = if signed {
            if negative {
                i64::MIN as u64
            } else {
                i64::MAX as u64
            }
        } else {
            u64::MAX
        };
    } else {
        scan.value = acc;
    }
    scan.consumed = index;
    scan
}

#[cfg(test)]
#[path = "scan_ut.rs"]
mod tests;
