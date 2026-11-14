// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 4: `strtol`, `strtoul` and `strtonum` (C41).
//!
//! The body compares both conversions with the independent `u128` model and checks the
//! invalid-base, range and consumed-length contracts.

use raven_libc_stdlib::{IntStatus, strtol, strtonum, strtoul};

use crate::model;
use crate::{Cursor, alloc_probe};

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

/// Run target 4.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let base = i32::from(cursor.u8());
    let signed = cursor.u8() & 1 != 0;
    let input = cursor.take(64);

    let expected = model::int(input, base);

    if expected.invalid_base {
        let (value, consumed, status) = strtol(input, base);
        assert!(
            status == IntStatus::InvalidBase && consumed == 0 && value == 0,
            "P-strtol-1: an invalid base was not rejected with consumed 0"
        );
        let (value, consumed, status) = strtoul(input, base);
        assert!(
            status == IntStatus::InvalidBase && consumed == 0 && value == 0,
            "P-strtol-1: strtoul accepted an invalid base"
        );
    } else if expected.no_digits {
        let (_, consumed, status) = strtol(input, base);
        assert!(
            status == IntStatus::NoDigits && consumed == 0,
            "P-strtol-4: a no-digit input reported a conversion"
        );
    } else {
        let (value, consumed, status) = strtol(input, base);
        assert!(
            consumed <= input.len(),
            "P-strtol-4: strtol consumed past the input"
        );
        assert_eq!(consumed, expected.consumed, "P-strtol-4: strtol consumed");
        if status == IntStatus::Range {
            assert!(
                value == i64::MIN || value == i64::MAX,
                "P-strtol-3: Range without a saturated value"
            );
        }
        assert_eq!(value, expected.signed, "P-strtol-2: strtol value");

        let (value, consumed, status) = strtoul(input, base);
        assert!(
            consumed <= input.len(),
            "P-strtol-4: strtoul consumed past the input"
        );
        assert_eq!(consumed, expected.consumed, "P-strtol-4: strtoul consumed");
        if status == IntStatus::Range {
            assert_eq!(value, u64::MAX, "P-strtol-3: strtoul Range saturation");
        }
        assert_eq!(value, expected.unsigned, "P-strtol-2: strtoul value");
    }

    let _ = strtonum(input, -1000, 1000);
    let _ = signed;

    alloc_probe::note(hash(&[
        u64::from(base as u8),
        u64::from(u32::from(signed)),
        expected.signed as u64,
        expected.unsigned,
        expected.consumed as u64,
    ]));
}

#[cfg(test)]
#[path = "strtol_input_ut.rs"]
mod tests;
