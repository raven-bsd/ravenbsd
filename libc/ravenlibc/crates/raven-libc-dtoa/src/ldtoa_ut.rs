// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::*;
use crate::ctx::DtoaCtx;
use crate::{DigitMode, Kind, LongDouble, Rounding};

fn ld80_one() -> LongDouble {
    LongDouble([0, 0, 0, 0, 0, 0, 0, 0x80, 0xff, 0x3f, 0, 0, 0, 0, 0, 0])
}

#[cfg(target_arch = "x86_64")]
#[test]
fn ld80_shortest_digits() {
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 11_700];
    let d = ldtoa(
        &mut ctx,
        ld80_one(),
        DigitMode::Shortest.as_mode(),
        0,
        Rounding::Nearest,
        &mut out,
    )
    .unwrap();
    assert_eq!(d.kind, Kind::Finite);
    assert_eq!(&out[..d.len], b"1");
    assert_eq!(d.decpt, 1);
    assert!(!d.negative);
}

#[cfg(any(target_arch = "aarch64", target_arch = "riscv64", test))]
#[test]
fn ld128_one_layout() {
    let (be, bits, kind, negative) = decompose_ld128(&{
        let mut bytes = [0u8; 16];
        bytes[15] = 0x3f;
        bytes[14] = 0xff;
        LongDouble(bytes)
    });
    assert_eq!(kind, STRTOG_NORMAL);
    assert!(!negative);
    assert_eq!(be, -112);
    assert_eq!(bits[3] & 0x0001_0000, 0x0001_0000);
}

#[cfg(test)]
#[test]
fn ld128_shortest_digits_on_the_host() {
    let mut bytes = [0u8; 16];
    bytes[15] = 0x3f;
    bytes[14] = 0xff;
    let one = LongDouble(bytes);
    let mut ctx = DtoaCtx::new();
    let mut out = [0u8; 64];
    let d = ldtoa_ld128(
        &mut ctx,
        one,
        DigitMode::Shortest.as_mode(),
        0,
        Rounding::Nearest,
        &mut out,
    )
    .unwrap();
    assert_eq!(d.kind, Kind::Finite);
    assert_eq!(&out[..d.len], b"1");
    assert_eq!(d.decpt, 1);
    // A binary128 1.5 has the same shortest digits as every other format.
    bytes[13] = 0x80;
    let mut ctx = DtoaCtx::new();
    let d = ldtoa_ld128(
        &mut ctx,
        LongDouble(bytes),
        DigitMode::Shortest.as_mode(),
        0,
        Rounding::Nearest,
        &mut out,
    )
    .unwrap();
    assert_eq!(&out[..d.len], b"15");
    assert_eq!(d.decpt, 1);
}
