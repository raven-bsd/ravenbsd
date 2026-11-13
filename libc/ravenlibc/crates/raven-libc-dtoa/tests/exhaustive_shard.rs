// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One 2^16-pattern shard of the exhaustive f32 loop with recorded C golden data.
//!
//! The data in `tests/vectors/ftoa_shard0.txt` was produced once by the NetBSD `__gdtoa` (binary32
//! FPI, nearest) for bit patterns 0x0000_0000..=0x0000_ffff. It is ordinary test data, so this
//! shard runs without the C oracle library.

use raven_libc_dtoa::{C_NUMERIC, DigitMode, DtoaCtx, Kind, Rounding, ftoa, strtof};

const SHARD: &str = include_str!("vectors/ftoa_shard0.txt");

fn build_text(out: &mut [u8; 96], digits: &[u8], decpt: i32, negative: bool) -> usize {
    let mut n = 0usize;
    if negative {
        out[n] = b'-';
        n += 1;
    }
    if digits.is_empty() {
        out[n] = b'0';
        n += 1;
    } else {
        out[n] = digits[0];
        n += 1;
        if digits.len() > 1 {
            out[n] = b'.';
            n += 1;
            out[n..n + digits.len() - 1].copy_from_slice(&digits[1..]);
            n += digits.len() - 1;
        }
    }
    out[n] = b'e';
    n += 1;
    let exp = decpt - 1;
    if exp < 0 {
        out[n] = b'-';
        n += 1;
        n += push_u32(&mut out[n..], (-exp) as u32);
    } else {
        n += push_u32(&mut out[n..], exp as u32);
    }
    n
}

fn push_u32(out: &mut [u8], mut v: u32) -> usize {
    let mut tmp = [0u8; 10];
    let mut n = 0;
    loop {
        tmp[n] = b'0' + (v % 10) as u8;
        n += 1;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    for i in 0..n {
        out[i] = tmp[n - 1 - i];
    }
    n
}

#[test]
fn recorded_f32_shard_zero() {
    let mut ctx = DtoaCtx::new();
    let mut checked = 0usize;
    for line in SHARD.lines() {
        if line.starts_with('#') || line.is_empty() {
            continue;
        }
        let mut fields = line.split('\t');
        let bits = u32::from_str_radix(fields.next().expect("bits"), 16).expect("bits hex");
        let value = f32::from_bits(bits);
        for tag in ["s", "n"] {
            let field = fields.next().expect("field");
            let mut parts = field.split(':');
            let name = parts.next().expect("tag");
            assert_eq!(name, tag);
            let len: usize = parts.next().expect("len").parse().expect("len");
            if value == 0.0 {
                let mode = if tag == "s" {
                    DigitMode::Shortest
                } else {
                    DigitMode::SigDigits
                };
                let mut out = [0u8; 32];
                let d = ftoa(
                    &mut ctx,
                    value,
                    mode,
                    if tag == "s" { -1 } else { 9 },
                    Rounding::Nearest,
                    &mut out,
                )
                .expect("ftoa zero");
                assert_eq!(&out[..d.len], b"0", "zero digits for {bits:08x}");
                assert_eq!(d.decpt, 1);
                assert_eq!(d.kind, Kind::Zero);
                assert_eq!(d.negative, bits >> 31 != 0);
                checked += 1;
                continue;
            }
            if len == 0 {
                let mut out = [0u8; 32];
                let mode = if tag == "s" {
                    DigitMode::Shortest
                } else {
                    DigitMode::SigDigits
                };
                let d = ftoa(
                    &mut ctx,
                    value,
                    mode,
                    if tag == "s" { -1 } else { 9 },
                    Rounding::Nearest,
                    &mut out,
                )
                .expect("ftoa special");
                assert_eq!(d.len, 0, "special len for {bits:08x}");
                let expect = if value.is_nan() { Kind::Nan } else { Kind::Inf };
                assert_eq!(d.kind, expect, "special kind for {bits:08x}");
                assert_eq!(d.negative, bits >> 31 != 0, "special sign for {bits:08x}");
                checked += 1;
                continue;
            }
            let digits = parts.next().expect("digits").as_bytes();
            assert_eq!(digits.len(), len);
            let decpt: i32 = parts.next().expect("decpt").parse().expect("decpt");
            let mut out = [0u8; 32];
            let (mode, nd) = if tag == "s" {
                (DigitMode::Shortest, -1)
            } else {
                (DigitMode::SigDigits, 9)
            };
            let d = ftoa(&mut ctx, value, mode, nd, Rounding::Nearest, &mut out).expect("ftoa");
            assert_eq!(&out[..d.len], digits, "digits for {bits:08x} {tag}");
            assert_eq!(d.decpt, decpt, "decpt for {bits:08x} {tag}");
            assert_eq!(d.negative, bits >> 31 != 0, "sign for {bits:08x} {tag}");
            let mut text = [0u8; 96];
            let n = build_text(&mut text, digits, decpt, d.negative);
            let parsed =
                strtof(&mut ctx, &text[..n], &C_NUMERIC, Rounding::Nearest).expect("strtof");
            assert_eq!(
                parsed.value.to_bits(),
                bits,
                "round trip for {bits:08x} {tag}"
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 131_072, "recorded shard checks");
}
