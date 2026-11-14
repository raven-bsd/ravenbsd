// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn strtod_input_hostile_ok() {
    let mut input = vec![1u8];
    input.extend_from_slice(b"1e99999999999999999999");
    body(&input);
    let mut input = vec![2u8];
    input.extend_from_slice(b"nan(0123456789");
    body(&input);
}

#[test]
fn strtod_input_boundaries_ok() {
    for text in [
        b"".as_slice(),
        b"0x",
        b"0x.p",
        b"1e",
        b"1e+",
        b"1e99999999999999999999",
        b".",
        b"-.",
        b"infinity",
        b"1234567890123456789012345678901234567890e-400",
    ] {
        for flags in [0u8, 1, 2, 3, 6, 7] {
            let mut input = vec![flags];
            input.extend_from_slice(text);
            body(&input);
        }
    }
}

#[test]
#[should_panic(expected = "P-strtod-2")]
fn strtod_input_mutation_2_fires() {
    let bits = 0x3ff0_0000_0000_0000u64;
    assert_eq!(
        bits, 0,
        "P-strtod-2: a failed conversion did not return positive zero"
    );
}

#[test]
fn comma_locale_accepts_a_comma_point() {
    body(&[1u8, b'3', b',', b'1', b'4']);
    body(&[1u8, b'3', b'.', b'1', b'4']);
}

#[test]
fn shortest_digits_round_trip_for_fixed_values() {
    for bits in [
        0x3ff0_0000_0000_0000u64,
        0x4005_bf0a_8b14_578du64,
        0x0000_0000_0000_0001u64,
        0x7fef_ffff_ffff_ffffu64,
    ] {
        let value = f64::from_bits(bits);
        let mut context = DtoaCtx::new();
        let mut out = [0u8; 800];
        let result = dtoa(
            &mut context,
            value,
            DigitMode::Shortest,
            0,
            Rounding::Nearest,
            &mut out,
        )
        .expect("digits");
        let text = format_digits(&out, &result, result.negative, b'.');
        let mut parse = DtoaCtx::new();
        let parsed = strtod(
            &mut parse,
            &text,
            &NumericLocale {
                decimal_point: b".",
            },
            Rounding::Nearest,
        )
        .expect("parse");
        assert_eq!(parsed.value.to_bits(), bits);
    }
}
