// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! The `drand48` family.
//!
//! CLEANROOM from POSIX (CR-11): every BSD copy carries Martin Birgmeier's notice (class OTHER,
//! never read as code) and musl is only the differential oracle. The recurrence is
//! `X(n+1) = (a * X(n) + c) mod 2^48`; `drand48` builds the double from its bits, so no float
//! arithmetic runs on soft-float targets (T-9).

const MASK48: u64 = 0x0000_ffff_ffff_ffff;
const DEFAULT_A: u64 = 0x0000_0005_deec_e66d;
const DEFAULT_C: u16 = 0xb;
const DEFAULT_X: u64 = 0x1234_abcd_330e;

/// The 48-bit generator state and its multiplier/addend.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct Rand48 {
    /// The 48-bit state `X`.
    pub x: u64,
    /// The 48-bit multiplier `a`.
    pub a: u64,
    /// The addend `c`.
    pub c: u16,
}

impl Rand48 {
    /// The POSIX initial state: `a = 0x5DEECE66D`, `c = 0xB`, `X = 0x1234ABCD330E`.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            x: DEFAULT_X,
            a: DEFAULT_A,
            c: DEFAULT_C,
        }
    }

    /// One step of the recurrence; returns the new 48-bit state.
    pub fn step(&mut self) -> u64 {
        self.x = self.a.wrapping_mul(self.x).wrapping_add(u64::from(self.c)) & MASK48;
        self.x
    }

    /// `srand48(seed)`: `X = (seed << 16) | 0x330E` and the default `a`/`c` are restored.
    pub fn srand48(&mut self, seed: i32) {
        self.x = ((u64::from(seed as u32) << 16) | 0x330e) & MASK48;
        self.a = DEFAULT_A;
        self.c = DEFAULT_C;
    }

    /// `seed48(seed16v)`: store the three low words and return the previous three.
    pub fn seed48(&mut self, seed16v: &[u16; 3]) -> [u16; 3] {
        let previous = [
            (self.x & 0xffff) as u16,
            ((self.x >> 16) & 0xffff) as u16,
            ((self.x >> 32) & 0xffff) as u16,
        ];
        self.x =
            u64::from(seed16v[0]) | (u64::from(seed16v[1]) << 16) | (u64::from(seed16v[2]) << 32);
        self.x &= MASK48;
        previous
    }

    /// `lcong48(p)`: `X = p[0..3]`, `a = p[3..6]`, `c = p[6]`.
    pub fn lcong48(&mut self, p: &[u16; 7]) {
        self.x = (u64::from(p[0]) | (u64::from(p[1]) << 16) | (u64::from(p[2]) << 32)) & MASK48;
        self.a = u64::from(p[3]) | (u64::from(p[4]) << 16) | (u64::from(p[5]) << 32);
        self.c = p[6];
    }

    /// Use a caller's `[u16; 3]` seed, returning the new 48-bit state (the `erand48` step).
    pub fn step_with(&mut self, seed16v: &mut [u16; 3]) -> u64 {
        let mut x =
            u64::from(seed16v[0]) | (u64::from(seed16v[1]) << 16) | (u64::from(seed16v[2]) << 32);
        x = self.a.wrapping_mul(x).wrapping_add(u64::from(self.c)) & MASK48;
        seed16v[0] = (x & 0xffff) as u16;
        seed16v[1] = ((x >> 16) & 0xffff) as u16;
        seed16v[2] = ((x >> 32) & 0xffff) as u16;
        x
    }
}

impl Default for Rand48 {
    fn default() -> Self {
        Self::new()
    }
}

/// `srand48(seed)` on the caller's generator.
pub fn srand48(generator: &mut Rand48, seed: i32) {
    generator.srand48(seed);
}

/// `seed48(seed16v)` on the caller's generator.
pub fn seed48(generator: &mut Rand48, seed16v: &[u16; 3]) -> [u16; 3] {
    generator.seed48(seed16v)
}

/// `lcong48(p)` on the caller's generator.
pub fn lcong48(generator: &mut Rand48, p: &[u16; 7]) {
    generator.lcong48(p);
}

/// Convert a 48-bit value to `value / 2^48` using integer bit construction only (T-9).
#[must_use]
pub fn x_to_double(x: u64) -> f64 {
    let x = x & MASK48;
    if x == 0 {
        return 0.0;
    }
    let bits = 64u32.wrapping_sub(x.leading_zeros());
    let exponent = u64::from(bits).wrapping_add(974);
    let mantissa = (x << (53u32.wrapping_sub(bits))) & 0x000f_ffff_ffff_ffff;
    f64::from_bits((exponent << 52) | mantissa)
}

/// `drand48()`.
pub fn drand48(generator: &mut Rand48) -> f64 {
    let x = generator.step();
    x_to_double(x)
}

/// `erand48(seed16v)`.
pub fn erand48(generator: &mut Rand48, seed16v: &mut [u16; 3]) -> f64 {
    let x = generator.step_with(seed16v);
    x_to_double(x)
}

/// `lrand48()`: the top 31 bits.
pub fn lrand48(generator: &mut Rand48) -> i64 {
    let x = generator.step();
    (x >> 17) as i64
}

/// `nrand48(seed16v)`: the top 31 bits from the caller's seed.
pub fn nrand48(generator: &mut Rand48, seed16v: &mut [u16; 3]) -> i64 {
    let x = generator.step_with(seed16v);
    (x >> 17) as i64
}

/// `mrand48()`: the top 32 bits as a signed value.
pub fn mrand48(generator: &mut Rand48) -> i64 {
    let x = generator.step();
    i64::from((x >> 16) as i32)
}

/// `jrand48(seed16v)`: the top 32 bits as a signed value from the caller's seed.
pub fn jrand48(generator: &mut Rand48, seed16v: &mut [u16; 3]) -> i64 {
    let x = generator.step_with(seed16v);
    i64::from((x >> 16) as i32)
}

#[cfg(test)]
#[path = "rand48_ut.rs"]
mod tests;
