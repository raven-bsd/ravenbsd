// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Public value types of the formatting engines.
//!
//! The float types are bit carriers: no `f32`, `f64` or `long double` value appears in this
//! crate, because the engine must build for soft-float targets and an integer-only
//! instantiation must link no numeric code (`donor-map/04` 3.8, T5).

use raven_libc_ctype::MbMode;

/// Output capacity a [`crate::FloatFmt`] caller must provide for a binary64 result.
///
/// Equals `raven_libc_dtoa::MAX_DIGITS_F64` (RD-30); the oracle harness asserts the equality.
pub const DEC_BUF_F64: usize = 800;

/// Output capacity a [`crate::FloatFmt`] caller must provide for an x87 80-bit or
/// IEEE binary128 `long double` result. Equals `raven_libc_dtoa::MAX_DIGITS_LD` (RD-30).
pub const DEC_BUF_LD: usize = 11_700;

/// Maximum number of bytes of one `scanf` floating-point token (RD-31).
///
/// This is a documented deviation from ISO C: a longer sequence is consumed to its end but
/// only the first `SCAN_TOKEN_MAX` bytes are stored; the exponent is still parsed.
pub const SCAN_TOKEN_MAX: usize = 4096;

/// Positional-argument limit of the `printf` engine. `NL_ARGMAX` is 64 (`donor-map/04` O5).
pub const NL_ARGMAX: usize = 64;

/// Storage width of a `scanf` conversion or a `%n` store.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum StoreSize {
    /// `hh`: one byte.
    Hh,
    /// `h`: two bytes.
    H,
    /// No length modifier: one `int`.
    Int,
    /// `l` (or `z`/`t` in the donor's mapping): one `long`.
    L,
    /// `ll`: one `long long`.
    Ll,
    /// `j`: one `intmax_t`.
    J,
    /// `z`: one `size_t`.
    Z,
    /// `t`: one `ptrdiff_t`.
    T,
}

impl StoreSize {
    /// The number of bytes the store writes on every supported target (all LP64).
    #[must_use]
    pub const fn bytes(self) -> usize {
        match self {
            Self::Hh => 1,
            Self::H => 2,
            Self::Int => 4,
            Self::L | Self::Ll | Self::J | Self::Z | Self::T => 8,
        }
    }
}

/// The kind of a floating-point value crossing the hook boundary.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum FloatKind {
    /// IEEE binary32. `printf` never produces it (a `float` argument is promoted to
    /// `double`), but `scanf` `%f` stores one.
    F32,
    /// IEEE binary64. `printf` `%f`/`%e`/`%g`/`%a` without `L`; `scanf` `%lf`.
    F64,
    /// x87 80-bit on x86_64, IEEE binary128 on aarch64 and riscv64.
    LongDouble,
}

/// Raw bits plus the kind needed to interpret them.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct FloatBits {
    /// How to interpret [`FloatBits::bits`].
    pub kind: FloatKind,
    /// The little-endian memory image in the low bits: binary32 in bits 0..32, binary64 in
    /// bits 0..64, and a full `long double` image in bits 0..128.
    pub bits: u128,
}

/// Classification of the value behind a digit string.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum FloatClass {
    /// A finite non-zero value.
    Finite,
    /// Positive or negative zero.
    Zero,
    /// An infinity.
    Inf,
    /// A NaN.
    Nan,
}

/// The digit string and layout descriptor returned by a [`crate::FloatFmt`] hook.
///
/// The value is `0.d1d2... * 10^decpt` (for `1.5`: digits `"15"`, `decpt` 1). The bytes are
/// ASCII digits and the callee has already trimmed trailing zeros; the engine pads.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct DigitsOut {
    /// Number of digit bytes written to the caller's buffer.
    pub len: usize,
    /// Decimal exponent of the first digit.
    pub decpt: i32,
    /// The sign bit, including `-0.0` and negative NaN.
    pub neg: bool,
    /// The classification.
    pub class: FloatClass,
}

/// The locale-dependent numeric strings the engine may use.
///
/// The C locale is [`C_NUMERIC_FORMAT`]: decimal point `"."`, no thousands separator and no
/// grouping.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct NumericFormat<'a> {
    /// The `LC_NUMERIC` decimal-point string.
    pub decimal_point: &'a [u8],
    /// The `LC_NUMERIC` thousands separator.
    pub thousands_sep: &'a [u8],
    /// The `LC_NUMERIC` grouping string; `0xFF` terminates it as `CHAR_MAX` does in C.
    pub grouping: &'a [u8],
}

/// The `C`/`POSIX` numeric format.
pub const C_NUMERIC_FORMAT: NumericFormat<'static> = NumericFormat {
    decimal_point: b".",
    thousands_sep: b"",
    grouping: b"",
};

/// Policy for the `%n` conversion (`donor-map/04` D-S6, H-6, RD-35).
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum PctN {
    /// POSIX: store the count so far (the `libc.so.1` default).
    Store,
    /// Hardened profile: return [`FmtErr::PctNAbort`] before storing.
    Abort,
    /// Signal-safe formatter: return [`FmtErr::Invalid`] before storing.
    Reject,
}

/// Options of one `printf` call.
#[derive(Clone, Copy)]
pub struct FmtOptions<'a> {
    /// What `%n` does.
    pub pct_n: PctN,
    /// The `errno` value captured by the shell at entry, used by `%m`.
    pub serrno: i32,
    /// The numeric locale strings; [`C_NUMERIC_FORMAT`] in the C locale.
    pub numeric: NumericFormat<'a>,
    /// The multibyte mode selected by the calling thread's locale (`Byte` in C/POSIX).
    pub mode: MbMode,
}

impl Default for FmtOptions<'static> {
    fn default() -> Self {
        Self {
            pct_n: PctN::Store,
            serrno: 0,
            numeric: C_NUMERIC_FORMAT,
            mode: MbMode::Byte,
        }
    }
}

/// Why a formatting or scanning call failed.
///
/// The C-ABI shell maps each variant to its documented return value and `errno`
/// (`donor-map/04` 4.6).
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum FmtErr {
    /// `EINVAL`: bad positional use, an unsupported conversion where the donor fails,
    /// [`NoFloat`](crate::NoFloat) or [`PctN::Reject`].
    Invalid,
    /// `ENOMEM`: the float adapter's arena or a `%m` allocation failed.
    NoMem,
    /// `EOVERFLOW`: the output would exceed `INT_MAX`, or `swprintf` truncation.
    Overflow,
    /// `EILSEQ`: invalid multibyte or wide input.
    Encoding,
    /// The sink reported a write failure; the glue decides the `errno`.
    Sink,
    /// The hardened `%n` policy asked the shell to abort.
    PctNAbort,
    /// A dummy float hook was asked for digits; the engine prints `*float*`.
    FloatUnavailable,
}

/// Options of one `scanf` call.
#[derive(Clone, Copy)]
pub struct ScanOptions<'a> {
    /// The numeric locale strings (the decimal point is used by the float adapter).
    pub numeric: NumericFormat<'a>,
    /// The multibyte mode selected by the calling thread's locale.
    pub mode: MbMode,
}

impl Default for ScanOptions<'static> {
    fn default() -> Self {
        Self {
            numeric: C_NUMERIC_FORMAT,
            mode: MbMode::Byte,
        }
    }
}

/// Result of one `scav` call.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct ScanResult {
    /// The number of assignments. The shell returns this unless [`ScanResult::return_eof`].
    pub assigned: i32,
    /// The number of input characters consumed (used by `%n` and file-position bookkeeping).
    pub consumed: usize,
    /// True when nothing was assigned and the donor's `!matches` rule returns `EOF` (-1).
    pub return_eof: bool,
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "types_ut.rs"]
mod tests;
