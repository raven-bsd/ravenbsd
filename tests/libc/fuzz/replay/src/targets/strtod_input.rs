// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Target 3: `strtod` and `strtof` (C42).
//!
//! The body checks consumed-length, the zero-on-no-conversion rule, idempotence on the consumed
//! prefix and the shortest-digits round trip.

use raven_libc_dtoa::{DigitMode, DtoaCtx, Kind, NumericLocale, Rounding, dtoa, strtod, strtof};

use crate::{Cursor, alloc_probe};

fn rounding(bits: u8) -> Rounding {
    match bits & 3 {
        0 => Rounding::TowardZero,
        1 => Rounding::Nearest,
        2 => Rounding::Upward,
        _ => Rounding::Downward,
    }
}

fn hash(parts: &[u64]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325u64;
    for part in parts {
        value ^= *part;
        value = value.wrapping_mul(0x100_0000_01b3);
    }
    value
}

/// Run target 3.
pub fn body(data: &[u8]) {
    let mut cursor = Cursor::new(data);
    let flags = cursor.u8();
    let point = if flags & 1 == 0 { b'.' } else { b',' };
    let mode = rounding(flags >> 1);
    let locale = NumericLocale {
        decimal_point: core::slice::from_ref(&point),
    };
    let input = cursor.take(512);

    let mut context = DtoaCtx::new();
    let parsed = strtod(&mut context, input, &locale, mode);
    let mut observed_bits = 0u64;
    if let Ok(result) = parsed {
        assert!(
            result.consumed <= input.len(),
            "P-strtod-1: strtod consumed past the input"
        );
        observed_bits = result.value.to_bits();
        if result.consumed == 0 {
            assert_eq!(
                observed_bits, 0,
                "P-strtod-2: a failed conversion did not return positive zero"
            );
        } else {
            let prefix = input.get(..result.consumed).unwrap_or(&[]);
            let mut again = DtoaCtx::new();
            if let Ok(second) = strtod(&mut again, prefix, &locale, mode)
                && !second.value.is_nan()
                && !result.value.is_nan()
            {
                assert_eq!(
                    second.value.to_bits(),
                    result.value.to_bits(),
                    "P-strtod-3: the consumed prefix parsed to a different value"
                );
                assert_eq!(
                    second.consumed, result.consumed,
                    "P-strtod-3: the consumed prefix reported a different length"
                );
            }
        }
    }

    let mut context = DtoaCtx::new();
    let _ = strtof(&mut context, input, &locale, mode);

    if input.len() >= 8 {
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(input.get(..8).unwrap_or(&[0; 8]));
        let value = f64::from_le_bytes(bytes);
        if value.is_finite() {
            let mut context = DtoaCtx::new();
            let mut digits = [0u8; 64];
            if let Ok(result) = dtoa(
                &mut context,
                value,
                DigitMode::Shortest,
                0,
                Rounding::Nearest,
                &mut digits,
            ) && result.kind == Kind::Finite
                && result.len > 0
            {
                let text = format_digits(&digits, &result, result.negative, point);
                let mut parse_context = DtoaCtx::new();
                if let Ok(parsed) = strtod(&mut parse_context, &text, &locale, Rounding::Nearest) {
                    assert_eq!(
                        parsed.value.to_bits(),
                        value.to_bits(),
                        "P-strtod-4: the shortest digits did not parse back"
                    );
                }
            }
        }
    }

    alloc_probe::note(hash(&[observed_bits, u64::from(flags)]));
}

fn format_digits(
    out: &[u8],
    result: &raven_libc_dtoa::Digits,
    negative: bool,
    point: u8,
) -> Vec<u8> {
    let digits = out.get(..result.len).unwrap_or(&[]);
    let mut text = Vec::new();
    if negative {
        text.push(b'-');
    }
    if let Some(first) = digits.first() {
        text.push(*first);
    }
    if digits.len() > 1 {
        text.push(point);
        text.extend_from_slice(digits.get(1..).unwrap_or(&[]));
    }
    text.push(b'e');
    let exponent = i64::from(result.decpt) - 1;
    text.extend_from_slice(exponent.to_string().as_bytes());
    text
}

#[cfg(test)]
#[path = "strtod_input_ut.rs"]
mod tests;
