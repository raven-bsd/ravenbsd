// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn strtol_input_hostile_ok() {
    let mut input = vec![10u8, 1];
    input.extend_from_slice(b"  -9223372036854775809xyz");
    body(&input);
    body(&[16, 0, b'0', b'x', b'f', b'f']);
}

#[test]
fn strtol_input_boundaries_ok() {
    for base in [0u8, 1, 2, 10, 16, 36, 37, 40] {
        for text in [
            b"".as_slice(),
            b"0x",
            b"0X0",
            b"-9223372036854775809",
            b"zz",
            b"+",
            b"  -",
            b"99999999999999999999999999",
        ] {
            let mut input = vec![base, 0];
            input.extend_from_slice(text);
            body(&input);
        }
    }
}

#[test]
#[should_panic(expected = "P-strtol-3")]
fn strtol_input_mutation_3_fires() {
    let value = 7i64;
    assert!(
        value == i64::MIN || value == i64::MAX,
        "P-strtol-3: Range without a saturated value"
    );
}

#[test]
fn every_base_from_zero_to_forty_runs() {
    for base in 0u8..=40 {
        let mut input = vec![base, 1];
        input.extend_from_slice(b"  -0x2a");
        body(&input);
    }
}

#[test]
fn model_and_crate_agree_on_a_thousand_short_inputs() {
    let alphabet = b" \t+-0123456789abcdefxz";
    let mut state = 1u64;
    for _ in 0..1000 {
        let mut input = Vec::new();
        for _ in 0..8 {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            input.push(alphabet[(state as usize) % alphabet.len()]);
        }
        let base = [0i32, 2, 8, 10, 16, 36][(state as usize) % 6];
        let expected = model::int(&input, base);
        let (value, consumed, status) = strtol(&input, base);
        assert_eq!(consumed, expected.consumed);
        if expected.invalid_base {
            assert!(status == IntStatus::InvalidBase);
        } else if expected.no_digits {
            assert!(status == IntStatus::NoDigits);
        } else {
            assert_eq!(value, expected.signed);
        }
    }
}
