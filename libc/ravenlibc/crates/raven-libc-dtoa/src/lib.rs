// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Correctly rounded float/decimal conversions: the `strtod`, `strtof` and `strtold` families and the
//! `dtoa`, `ldtoa`, `hdtoa` and `hldtoa` digit generators.
//!
//! `architecture/raven-libc/donor-map/05-numerics.md` sections 3.1-3.2 are the contract. Every
//! conversion takes a [`DtoaCtx`] the caller places on its stack; the crate has no global state, no
//! lock, no `errno`, no locale lookup and no `unsafe`. Results are digits in the caller's buffer
//! (ASCII, no NUL, trailing zeros suppressed) plus a [`Digits`] or [`HexDigits`] descriptor.
//!
//! The arithmetic is a faithful transpile of David Gay's gdtoa from NetBSD `lib/libc/gdtoa`; the
//! file headers carry the per-file provenance. `printf`/`scanf` live in `raven-libc-fmt` (C43) and
//! never here.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects,
    clippy::unreachable,
    clippy::todo,
    clippy::unimplemented
)]

#[cfg(feature = "alloc")]
extern crate alloc;

pub mod bigint;
pub mod config;
pub mod ctx;
pub mod dtoa;
#[cfg(any(target_arch = "aarch64", target_arch = "riscv64", test))]
pub mod fmt_ld128;
#[cfg(target_arch = "x86_64")]
pub mod fmt_ld80;
pub mod gdtoa;
pub mod gethex;
pub mod hdtoa;
pub mod hexdig;
pub mod hexnan;
pub mod ldtoa;
pub mod strtod;
pub mod strtodg;
pub mod strtof;
pub mod strtold;
pub mod strtord;
pub mod types;
pub mod ulp;

pub use ctx::DtoaCtx;

/// FLT_ROUNDS numbering (NOT the `FE_*` values): the shell maps `fegetround()` once per call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum Rounding {
    /// Toward zero (FLT_ROUNDS 0).
    TowardZero = 0,
    /// To nearest, ties to even (FLT_ROUNDS 1).
    Nearest = 1,
    /// Toward +infinity (FLT_ROUNDS 2).
    Upward = 2,
    /// Toward -infinity (FLT_ROUNDS 3).
    Downward = 3,
}

impl Rounding {
    /// The FLT_ROUNDS / `FPI_Round_*` number.
    #[must_use]
    pub(crate) const fn as_fpi(self) -> i32 {
        self as i32
    }
}

/// Everything locale-dependent the conversion may know. Never empty; the C locale is [`C_NUMERIC`].
#[derive(Clone, Copy)]
pub struct NumericLocale<'a> {
    /// The `LC_NUMERIC` decimal-point string (one byte for `C`, several for UTF-8 locales).
    pub decimal_point: &'a [u8],
}

/// The `C` locale's numeric separator.
pub const C_NUMERIC: NumericLocale<'static> = NumericLocale {
    decimal_point: b".",
};

/// Digit-generation modes. Gay's debug modes 6..9 are not exposed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(u8)]
pub enum DigitMode {
    /// Shortest string that round-trips (round-to-nearest reading).
    Shortest = 0,
    /// Shortest string with the Steele-White stopping rule.
    ShortestSteele = 1,
    /// `max(1, ndigits)` significant digits, trailing zeros suppressed.
    SigDigits = 2,
    /// `ndigits` digits past the point (may be negative), trailing zeros suppressed.
    Decimals = 3,
    /// Like [`DigitMode::SigDigits`], may return a shorter round-tripping string.
    SigDigitsShort = 4,
    /// Like [`DigitMode::Decimals`], may return a shorter round-tripping string.
    DecimalsShort = 5,
}

impl DigitMode {
    /// The donor's `mode` number.
    #[must_use]
    pub(crate) const fn as_mode(self) -> i32 {
        self as i32
    }
}

/// Classification of the value behind a digit string.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// Both zeros.
    Zero,
    /// A finite non-zero value.
    Finite,
    /// An infinity.
    Inf,
    /// A NaN.
    Nan,
}

/// Result of [`fn@dtoa`], [`fn@ldtoa`] and [`fn@ftoa`].
///
/// The value is `0.d1d2... * 10^decpt` (for 1.5: digits `"15"`, `decpt` 1). `len` is the number of
/// digit bytes written to the caller's buffer; the bytes are ASCII `'0'..='9'` without a NUL.
/// `Zero` returns one digit `"0"` and `decpt` 1; `Inf`/`Nan` return `len == 0`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Digits {
    /// Number of digit bytes written.
    pub len: usize,
    /// Decimal exponent of the first digit.
    pub decpt: i32,
    /// The sign bit of the value, including `-0.0` and negative NaN.
    pub negative: bool,
    /// The classification.
    pub kind: Kind,
}

/// Result of [`fn@hdtoa`] and [`fn@hldtoa`].
///
/// The digits are hexadecimal (`'0'..='9'` and `abcdef` or `ABCDEF`); the engine prints
/// `0x<digits>p<exp2>` with a point after the first digit. `exp2` is the binary exponent printed
/// after `p` (in the donor's convention: for ld80 the leading nibble carries the x87 explicit
/// integer bit, so the value is reconstructed as written). `Inf`/`Nan` return `len == 0`; `Zero`
/// returns `"0"` and `exp2 == 0`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct HexDigits {
    /// Number of digit bytes written.
    pub len: usize,
    /// Binary exponent printed after `p`.
    pub exp2: i32,
    /// The sign bit of the value.
    pub negative: bool,
    /// The classification.
    pub kind: Kind,
}

/// Why a conversion could not produce digits.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DtoaError {
    /// The caller's output buffer is smaller than the donor's result allocation.
    BufferTooSmall {
        /// The number of digit bytes the conversion needs.
        needed: usize,
    },
    /// The per-call arena is exhausted and the `alloc` feature is off (or allocation failed).
    NoMemory,
}

/// Range indications of a `strto*` conversion; the shell sets `errno = ERANGE` on `OVERFLOW` or
/// `UNDERFLOW`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct RangeFlags(u32);

impl RangeFlags {
    /// The magnitude was too large for the target format.
    pub const OVERFLOW: RangeFlags = RangeFlags(0x01);
    /// The result underflowed to zero or is denormal.
    pub const UNDERFLOW: RangeFlags = RangeFlags(0x02);
    /// The returned value is not exact.
    pub const INEXACT: RangeFlags = RangeFlags(0x04);

    /// The union of two flag sets.
    #[must_use]
    pub const fn union(self, other: RangeFlags) -> RangeFlags {
        RangeFlags(self.0 | other.0)
    }

    /// Whether every flag of `other` is present.
    #[must_use]
    pub const fn contains(self, other: RangeFlags) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether no flag is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The raw bit set (bit 0 overflow, bit 1 underflow, bit 2 inexact).
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }
}

/// Raw little-endian memory image of a C `long double`.
///
/// x86_64: bytes 0..10 are meaningful (x87 80-bit extended), bytes 10..16 are zero on output and
/// ignored on input. aarch64 and riscv64: all 16 bytes are IEEE binary128.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct LongDouble(pub [u8; 16]);

/// A parsed floating-point value plus the parse extent and range flags.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct ParseResult<T> {
    /// The converted value.
    pub value: T,
    /// Bytes consumed from the start of the input; 0 means no conversion (the shell leaves
    /// `endptr` at the original pointer, the value is `+0.0` and `errno` is untouched).
    pub consumed: usize,
    /// Overflow/underflow/inexact indications.
    pub range: RangeFlags,
}

/// The per-call arena size in bytes (RD-30): 8 KiB, enough for an 800-digit `strtod` and every
/// `double` conversion without spilling.
pub const POOL_BYTES: usize = 8192;

/// Published output capacity for `ftoa`/`hdtoa` callers (RD-30).
pub const MAX_DIGITS_F32: usize = 128;

/// Published output capacity for `dtoa`/`hdtoa` callers (RD-30).
pub const MAX_DIGITS_F64: usize = 800;

/// Published output capacity for `ldtoa`/`hldtoa` callers (RD-30).
pub const MAX_DIGITS_LD: usize = 11_700;

/// Convert `v` to decimal digits.
///
/// # Errors
/// [`DtoaError::BufferTooSmall`] when `out` cannot hold the donor's result allocation and
/// [`DtoaError::NoMemory`] when the arena is exhausted.
pub fn dtoa(
    ctx: &mut DtoaCtx,
    v: f64,
    mode: DigitMode,
    ndigits: i32,
    r: Rounding,
    out: &mut [u8],
) -> Result<Digits, DtoaError> {
    dtoa::dtoa(ctx, v, mode.as_mode(), ndigits, r, out)
}

/// Convert a C `long double` image to decimal digits (see [`fn@dtoa`]).
///
/// # Errors
/// As [`fn@dtoa`].
pub fn ldtoa(
    ctx: &mut DtoaCtx,
    v: LongDouble,
    mode: DigitMode,
    ndigits: i32,
    r: Rounding,
    out: &mut [u8],
) -> Result<Digits, DtoaError> {
    ldtoa::ldtoa(ctx, v, mode.as_mode(), ndigits, r, out)
}

/// Convert an `f32` to decimal digits through the generic binary32 `gdtoa` core (NEW in C42; the
/// exhaustive f32 test uses it, the printf hook does not).
///
/// # Errors
/// As [`fn@dtoa`].
pub fn ftoa(
    ctx: &mut DtoaCtx,
    v: f32,
    mode: DigitMode,
    ndigits: i32,
    r: Rounding,
    out: &mut [u8],
) -> Result<Digits, DtoaError> {
    gdtoa::ftoa(ctx, v, mode.as_mode(), ndigits, r, out)
}

/// Convert `v` to hexadecimal digits for `printf %a`.
///
/// `ndigits < 0` means "as many digits as needed"; `upper` selects `0123456789ABCDEF`.
///
/// # Errors
/// As [`fn@dtoa`].
pub fn hdtoa(
    v: f64,
    upper: bool,
    ndigits: i32,
    r: Rounding,
    out: &mut [u8],
) -> Result<HexDigits, DtoaError> {
    hdtoa::hdtoa(v, upper, ndigits, r, out)
}

/// Convert a C `long double` image to hexadecimal digits for `printf %La`.
///
/// # Errors
/// As [`fn@dtoa`].
pub fn hldtoa(
    v: LongDouble,
    upper: bool,
    ndigits: i32,
    r: Rounding,
    out: &mut [u8],
) -> Result<HexDigits, DtoaError> {
    hdtoa::hldtoa(v, upper, ndigits, r, out)
}

/// Parse a decimal/hex/inf/nan string into an `f64`.
///
/// # Errors
/// [`DtoaError::NoMemory`] when the arena is exhausted.
pub fn strtod(
    ctx: &mut DtoaCtx,
    s: &[u8],
    loc: &NumericLocale,
    r: Rounding,
) -> Result<ParseResult<f64>, DtoaError> {
    let (value, consumed, flags) = strtod::strtod(ctx, s, loc, r)?;
    Ok(ParseResult {
        value,
        // C70: the donor's end pointer can step over the terminating NUL; a slice has no
        // such byte, so a conversion never consumes past its input.
        consumed: consumed.min(s.len()),
        range: range_from(flags),
    })
}

/// Parse a decimal/hex/inf/nan string into an `f32` (correctly rounded; never via `f64`).
///
/// # Errors
/// [`DtoaError::NoMemory`] when the arena is exhausted.
pub fn strtof(
    ctx: &mut DtoaCtx,
    s: &[u8],
    loc: &NumericLocale,
    r: Rounding,
) -> Result<ParseResult<f32>, DtoaError> {
    let (value, consumed, flags) = strtof::strtof(ctx, s, loc, r)?;
    Ok(ParseResult {
        value,
        // C70: see `strtod`; the end pointer never passes the slice end.
        consumed: consumed.min(s.len()),
        range: range_from(flags),
    })
}

/// Parse a decimal/hex/inf/nan string into a C `long double` image.
///
/// # Errors
/// [`DtoaError::NoMemory`] when the arena is exhausted.
pub fn strtold(
    ctx: &mut DtoaCtx,
    s: &[u8],
    loc: &NumericLocale,
    r: Rounding,
) -> Result<ParseResult<LongDouble>, DtoaError> {
    let (value, consumed, flags) = strtold::strtold(ctx, s, loc, r)?;
    Ok(ParseResult {
        value,
        consumed,
        range: range_from(flags),
    })
}

/// Map donor `STRTOG_*` flags to the public range set.
pub(crate) fn range_from(flags: i32) -> RangeFlags {
    let mut range = RangeFlags::default();
    if flags & types::STRTOG_OVERFLOW != 0 {
        range = range.union(RangeFlags::OVERFLOW);
    }
    if flags & types::STRTOG_UNDERFLOW != 0 {
        range = range.union(RangeFlags::UNDERFLOW);
    }
    if flags & types::STRTOG_INEXACT != 0 {
        range = range.union(RangeFlags::INEXACT);
    }
    range
}

/// Test-only generic parser entry: drives [`types::Fpi`] formats (`ld80`, `ld128`) from the unit
/// tests, including the binary128 property tests on the x86_64 host (5.4a).
#[cfg(test)]
pub fn strtodg_test(
    ctx: &mut DtoaCtx,
    s: &[u8],
    fpi: &types::Fpi,
    expt: &mut i32,
    bits: &mut [u32],
    loc: &NumericLocale,
) -> Result<(usize, i32), DtoaError> {
    strtodg::strtodg(ctx, s, fpi, expt, bits, loc)
}
