// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! 100,000 golden `strtod` results generated once from the NetBSD gdtoa C original.
//!
//! The file is data (see `tests/vectors/golden.txt`); the same file is intended for the
//! soft-float targets, where it will be re-run by a later ticket. This host test proves that the
//! Rust port reproduces the C value, the consumed count and the ERANGE indication bit for bit.

use raven_libc_dtoa::{C_NUMERIC, DtoaCtx, RangeFlags, Rounding, strtod};

const GOLDEN: &str = include_str!("vectors/golden.txt");

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex byte"))
        .collect()
}

#[test]
fn strtod_golden_vectors() {
    let mut checked = 0usize;
    for line in GOLDEN.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 4, "golden field count: {line}");
        let input = unhex(fields[0]);
        let bits = u64::from_str_radix(fields[1], 16).expect("value bits");
        let consumed: usize = fields[2].parse().expect("consumed");
        let erange = fields[3] == "1";
        let mut ctx = DtoaCtx::new();
        let parsed = strtod(&mut ctx, &input, &C_NUMERIC, Rounding::Nearest).expect("strtod");
        assert_eq!(
            parsed.value.to_bits(),
            bits,
            "golden value for {:?}",
            String::from_utf8_lossy(&input)
        );
        assert_eq!(
            parsed.consumed,
            consumed,
            "golden consumed for {:?}",
            String::from_utf8_lossy(&input)
        );
        let got_erange = parsed.range.contains(RangeFlags::OVERFLOW)
            || parsed.range.contains(RangeFlags::UNDERFLOW);
        assert_eq!(
            got_erange,
            erange,
            "golden ERANGE for {:?}",
            String::from_utf8_lossy(&input)
        );
        checked += 1;
    }
    assert_eq!(checked, 100_000, "golden vector count");
}
