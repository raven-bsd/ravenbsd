// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Exact-reference vectors for binary32 directed rounding.
//!
//! The NetBSD `strtodg.c` directed path is defective for `nbits = 24`: its boundary test passes
//! the live `rvb->x[i]` to `lo0bits()`, which shifts its argument in place, so round-zero and
//! round-down can return a truncated significand (`5e-31` -> `0x00051212` instead of
//! `0x00a2425f`). The vectors in `tests/vectors/directed_f32.tsv` were generated with Python's
//! exact `fractions` module, independently of both the C donor and this crate.

use raven_libc_dtoa::{C_NUMERIC, DtoaCtx, RangeFlags, Rounding, strtof};

const VECTORS: &str = include_str!("vectors/directed_f32.tsv");

fn parse_hex(text: &str) -> u32 {
    u32::from_str_radix(text, 16).expect("hex bits")
}

#[test]
fn directed_f32_exact_vectors() {
    let mut checked = 0usize;
    for line in VECTORS.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        assert_eq!(fields.len(), 6, "vector field count: {line}");
        let text = fields[0].as_bytes();
        let expected = [
            (Rounding::Nearest, parse_hex(fields[1])),
            (Rounding::Upward, parse_hex(fields[2])),
            (Rounding::Downward, parse_hex(fields[3])),
            (Rounding::TowardZero, parse_hex(fields[4])),
        ];
        let consumed: usize = fields[5].parse().expect("consumed");
        for (mode, bits) in expected {
            let mut ctx = DtoaCtx::new();
            let parsed = strtof(&mut ctx, text, &C_NUMERIC, mode).expect("strtof");
            assert_eq!(
                parsed.value.to_bits(),
                bits,
                "strtof {:?} mode {mode:?}",
                String::from_utf8_lossy(text)
            );
            assert_eq!(parsed.consumed, consumed, "consumed for {line}");
            assert!(
                !parsed.range.contains(RangeFlags::OVERFLOW)
                    && !parsed.range.contains(RangeFlags::UNDERFLOW),
                "unexpected range flag for {line}"
            );
            checked += 1;
        }
    }
    assert!(checked >= 80_000, "only {checked} directed checks ran");
}
