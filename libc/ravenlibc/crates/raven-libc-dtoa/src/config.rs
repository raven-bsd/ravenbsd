// SPDX-License-Identifier: LicenseRef-gdtoa
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (C) 1998-2000 by Lucent Technologies
// All Rights Reserved
//
// RavenBSD: transpiled from netbsd:lib/libc/gdtoa/gdtoaimp.h @ 36eddebcc36cb0775aa2962005b9c05419a6ff01
// (file revision tag: $NetBSD: gdtoaimp.h,v 1.19 2022/04/19 20:32:15 rillig Exp $)
//
// The author of this software is David M. Gay.
//
// Permission to use, copy, modify, and distribute this software and
// its documentation for any purpose and without fee is hereby
// granted, provided that the above copyright notice appear in all
// copies and that both that the copyright notice and this
// permission notice and warranty disclaimer appear in supporting
// documentation, and that the name of Lucent or any of its entities
// not be used in advertising or publicity pertaining to
// distribution of the software without specific, written prior
// permission.
//
// LUCENT DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE,
// INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS.
// IN NO EVENT SHALL LUCENT OR ANY OF ITS ENTITIES BE LIABLE FOR ANY
// SPECIAL, INDIRECT OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER
// IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
// ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF
// THIS SOFTWARE.
//
// Please send bug reports to David M. Gay (dmg at acm dot org,
// with " at " changed at "@" and " dot " changed to ".").
//! The compile-time configuration of `gdtoaimp.h` for the three little-endian targets.
//!
//! This replaces every `#define` of NetBSD gdtoa that selects an arithmetic or an allocation
//! strategy: little-endian IEEE only, 32-bit `ULong` words, `Kmax` 9 (RD-31), no `errno`, no
//! threads, no locale lookup, no global pool. The arch-specific NaN patterns are CLEANROOM
//! constants (donor-map/05 3.1; `crate::config_ut` checks them).

/// Words per `ULong` (Pack_32).
pub const UL_BITS: u32 = 32;
/// log2 of `UL_BITS`.
pub const KSHIFT: u32 = 5;
/// `UL_BITS - 1`.
pub const KMASK: u32 = 31;
/// Every bit of a `ULong`.
pub const ALL_ON: u32 = 0xffff_ffff;

/// Donor `Kmax`: the largest size class with a two-word `Bigint` header (RD-31).
pub const KMAX: i32 = 9;
/// Largest size class the arena keeps a free list for (5.2: pathological long-double blocks).
pub const KBIG: i32 = 14;

/// Significand bits of `double`.
pub const P: i32 = 53;
/// `double` exponent bias.
pub const BIAS: i32 = 1023;
/// Exponent of the least-significant bit of a `double` denormal.
pub const EMIN: i32 = -1022;
/// `double` exponent field shift.
pub const EXP_SHIFT: u32 = 20;
/// Alias used by `gdtoa.c`/`strtodg.c`.
pub const EXP_SHIFT1: u32 = 20;
/// `double` exponent field mask.
pub const EXP_MASK: u32 = 0x7ff0_0000;
/// The implicit integer bit of a normal `double`.
pub const EXP_MSK1: u32 = 0x0010_0000;
/// Fraction mask of the high word.
pub const FRAC_MASK: u32 = 0x000f_ffff;
/// Alias used by the rounding paths.
pub const FRAC_MASK1: u32 = 0x000f_ffff;
/// Boundary mask used by the rounding decisions.
pub const BNDRY_MASK: u32 = 0x000f_ffff;
/// The IEEE sign bit.
pub const SIGN_BIT: u32 = 0x8000_0000;
/// High word of 1.0.
pub const EXP_1: u32 = 0x3ff0_0000;
/// Alias used by `gdtoa.c`/`strtod.c`.
pub const EXP_11: u32 = 0x3ff0_0000;
/// The least significant bit (`LSB`).
pub const LSB: u32 = 1;
/// `Log2P` (one bit of the integer part).
pub const LOG2P: i32 = 1;
/// `Tiny0` (low word of the smallest denormal).
pub const TINY0: u32 = 0;
/// `Tiny1` (high word of the smallest denormal).
pub const TINY1: u32 = 1;
/// `Bletch`: the largest power of two below `DBL_MAX_10_EXP`, over 16.
pub const BLETCH: i32 = 0x10;
/// `Quick_max`: largest digit count using the floating-point fast path.
pub const QUICK_MAX: i32 = 14;
/// `Int_max`: largest decimal exponent using the "small integer" path.
pub const INT_MAX: i32 = 14;
/// `Ten_pmax`: largest k with an exact `10^k`.
pub const TEN_PMAX: i32 = 22;
/// Number of entries of `BIGTENS`/`TINYTENS`.
pub const N_BIGTENS: usize = 5;
/// `DBL_DIG`.
pub const DBL_DIG: i32 = 15;
/// `DBL_MAX_10_EXP`.
pub const DBL_MAX_10_EXP: i32 = 308;
/// `DBL_MAX_EXP`.
pub const DBL_MAX_EXP: i32 = 1024;
/// `FLT_RADIX`.
pub const FLT_RADIX: f64 = 2.0;

/// `Big0`: the largest finite `double`'s high word.
pub const BIG0: u32 = 0x7fef_ffff;
/// `Big1`: the largest finite `double`'s low word.
pub const BIG1: u32 = 0xffff_ffff;

/// Canonical quiet NaN of binary32 (CLEANROOM; arch/{x86_64,aarch64,riscv}/gdtoa/gd_qnan.h).
pub const F_QNAN: u32 = 0x7fc0_0000;
/// Canonical quiet NaN of binary64, high word (memory order).
pub const D_QNAN0: u32 = 0x0000_0000;
/// Canonical quiet NaN of binary64, low word (memory order).
pub const D_QNAN1: u32 = 0x7ff8_0000;
/// Canonical quiet NaN of x87 80-bit extended, memory order.
pub const LDUS_QNAN: [u16; 5] = [0x0000, 0x0000, 0x0000, 0xc000, 0x7fff];
/// Canonical quiet NaN of IEEE binary128, memory order.
pub const LD_QNAN: [u32; 4] = [0x0000_0000, 0x0000_0000, 0x0000_0000, 0x7fff_8000];

/// `Scale_Bit`: set in the binary exponent of `e1` for `Avoid_Underflow`.
pub const SCALE_BIT: i32 = 0x10;

/// `tens[]` of `misc.c`: exactly representable powers of ten, as bits.
pub const TENS: [f64; 23] = [
    f64::from_bits(0x3ff0_0000_0000_0000),
    f64::from_bits(0x4024_0000_0000_0000),
    f64::from_bits(0x4059_0000_0000_0000),
    f64::from_bits(0x408f_4000_0000_0000),
    f64::from_bits(0x40c3_8800_0000_0000),
    f64::from_bits(0x40f8_6a00_0000_0000),
    f64::from_bits(0x412e_8480_0000_0000),
    f64::from_bits(0x4163_12d0_0000_0000),
    f64::from_bits(0x4197_d784_0000_0000),
    f64::from_bits(0x41cd_cd65_0000_0000),
    f64::from_bits(0x4202_a05f_2000_0000),
    f64::from_bits(0x4237_4876_e800_0000),
    f64::from_bits(0x426d_1a94_a200_0000),
    f64::from_bits(0x42a2_309c_e540_0000),
    f64::from_bits(0x42d6_bcc4_1e90_0000),
    f64::from_bits(0x430c_6bf5_2634_0000),
    f64::from_bits(0x4341_c379_37e0_8000),
    f64::from_bits(0x4376_3457_85d8_a000),
    f64::from_bits(0x43ab_c16d_674e_c800),
    f64::from_bits(0x43e1_58e4_6091_3d00),
    f64::from_bits(0x4415_af1d_78b5_8c40),
    f64::from_bits(0x444b_1ae4_d6e2_ef50),
    f64::from_bits(0x4480_f0cf_064d_d592),
];

/// `bigtens[]` of `misc.c`.
pub const BIGTENS: [f64; 5] = [
    f64::from_bits(0x4341_c379_37e0_8000),
    f64::from_bits(0x4693_b8b5_b505_6e17),
    f64::from_bits(0x4d38_4f03_e93f_f9f5),
    f64::from_bits(0x5a82_7748_f930_1d32),
    f64::from_bits(0x7515_4fdd_7f73_bf3c),
];

/// `tinytens[]` of `misc.c`.
pub const TINYTENS: [f64; 5] = [
    f64::from_bits(0x3c9c_d2b2_97d8_89bc),
    f64::from_bits(0x3949_f623_d5a8_a733),
    f64::from_bits(0x32a5_0ffd_44f4_a73d),
    f64::from_bits(0x255b_ba08_cf8c_979d),
    f64::from_bits(0x0ac8_0628_64ac_6f43),
];

/// `tinytens[]` of `strtod.c` under `Avoid_Underflow` (entry 4 is scaled by `2^106`).
pub const STRTO_TINYTENS: [f64; 5] = [
    f64::from_bits(0x3c9c_d2b2_97d8_89bc),
    f64::from_bits(0x3949_f623_d5a8_a733),
    f64::from_bits(0x32a5_0ffd_44f4_a73d),
    f64::from_bits(0x255b_ba08_cf8c_979d),
    f64::from_bits(0x1168_0628_64ac_6f43),
];

/// The C `word0`: the HIGH 32 bits on a little-endian IEEE machine (gdtoaimp.h `_0 = 1`).
#[must_use]
pub const fn word0(x: f64) -> u32 {
    (x.to_bits() >> 32) as u32
}

/// The C `word1`: the LOW 32 bits on a little-endian IEEE machine.
#[must_use]
pub const fn word1(x: f64) -> u32 {
    x.to_bits() as u32
}

/// Assign `word0` without touching the low word.
pub fn set_word0(x: &mut f64, value: u32) {
    *x = f64::from_bits((x.to_bits() & 0xffff_ffff) | ((value as u64) << 32));
}

/// `word0(x) &= mask`.
pub fn w0_and(x: &mut f64, mask: u32) {
    let value = word0(*x) & mask;
    set_word0(x, value);
}

/// `word0(x) |= bits`.
pub fn w0_or(x: &mut f64, bits: u32) {
    let value = word0(*x) | bits;
    set_word0(x, value);
}

/// `word0(x) += value` with the donor's unsigned wrap.
pub fn w0_add(x: &mut f64, value: u32) {
    let result = word0(*x).wrapping_add(value);
    set_word0(x, result);
}

/// `word0(x) -= value` with the donor's unsigned wrap.
pub fn w0_sub(x: &mut f64, value: u32) {
    let result = word0(*x).wrapping_sub(value);
    set_word0(x, result);
}

/// `word1(x) &= mask`.
pub fn w1_and(x: &mut f64, mask: u32) {
    let result = word1(*x) & mask;
    set_word1(x, result);
}

/// Assign `word1` without touching the high word.
pub fn set_word1(x: &mut f64, value: u32) {
    *x = f64::from_bits((x.to_bits() & 0xffff_ffff_0000_0000) | (value as u64));
}

/// Build a `double` from its two words (the donor's `U`).
#[must_use]
pub const fn from_words(w0: u32, w1: u32) -> f64 {
    f64::from_bits(((w0 as u64) << 32) | (w1 as u64))
}

/// The C string terminator: the end of the slice reads as `0`.
#[must_use]
pub fn at(s: &[u8], i: usize) -> u8 {
    match s.get(i) {
        Some(byte) => *byte,
        None => 0,
    }
}

/// A `Long` from a `usize` count, saturating instead of wrapping (the donor's counts are small).
#[must_use]
pub fn long_from(count: usize) -> i32 {
    i32::try_from(count).unwrap_or(i32::MAX)
}

#[cfg(test)]
#[path = "config_ut.rs"]
mod tests;
