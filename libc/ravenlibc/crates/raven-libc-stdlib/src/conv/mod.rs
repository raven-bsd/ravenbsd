// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! Integer conversion: the shared [`scan_integer`] core, `strtol`/`strtoul` and their LP64
//! aliases, `strtonum`, `atoi`/`atol`/`atoll`.
//!
//! The `strtoll`, `strtoimax`, `strtoull` and `strtoumax` names are aliases of the one `i64` or
//! `u64` instantiation, as R-10 requires; the OpenBSD files that define them are not transpiled
//! separately.

use crate::CodeUnit;

pub mod atoi;
pub mod atol;
pub mod scan;
pub mod strtol;
pub mod strtonum;
pub mod strtoul;

pub use atoi::atoi;
pub use atol::{atol, atoll};
pub use scan::scan_integer;
pub use strtol::strtol;
pub use strtonum::{StrtonumError, strtonum};
pub use strtoul::strtoul;

/// The outcome of a scan besides the value and the consumed byte count.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum IntStatus {
    /// A number was converted without overflow.
    Ok,
    /// The magnitude overflowed the signed or unsigned range; the value is saturated.
    Range,
    /// `strtol`-family base `< 0`, `1` or `> 36`.
    InvalidBase,
    /// No digit was consumed; `consumed` is 0 and `errno` is not changed.
    NoDigits,
}

/// The shared scan result.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct Scan {
    /// The magnitude, or the saturated signed/unsigned limit on [`IntStatus::Range`].
    pub value: u64,
    /// Whether the input carried a `-` sign.
    pub negative: bool,
    /// Bytes consumed from the start of `s`; 0 when nothing was converted.
    pub consumed: usize,
    /// Conversion status.
    pub status: IntStatus,
}

/// `strtoll`: the `i64` `strtol` body.
#[must_use]
pub fn strtoll(s: &[u8], base: i32) -> (i64, usize, IntStatus) {
    strtol(s, base)
}

/// `strtoimax`: the `i64` `strtol` body.
#[must_use]
pub fn strtoimax(s: &[u8], base: i32) -> (i64, usize, IntStatus) {
    strtol(s, base)
}

/// `strtoull`: the `u64` `strtoul` body.
#[must_use]
pub fn strtoull(s: &[u8], base: i32) -> (u64, usize, IntStatus) {
    strtoul(s, base)
}

/// `strtoumax`: the `u64` `strtoul` body.
#[must_use]
pub fn strtoumax(s: &[u8], base: i32) -> (u64, usize, IntStatus) {
    strtoul(s, base)
}

/// Scan an integer from `s` with `base`; the wide family reuses this with `u32` code units.
#[must_use]
pub fn scan<C: CodeUnit>(s: &[C], base: i32, signed: bool) -> Scan {
    scan_integer(s, base, signed)
}

#[cfg(test)]
#[path = "mod_ut.rs"]
mod tests;
