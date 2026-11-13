// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The trait seam between the engines and the C-ABI shell.
//!
//! `Sink`/`ScanSource` are the output and input side, `VaReader` is the real `va_list`
//! reader (C59), `FmtEnv` owns every operation that dereferences a C pointer, and
//! `FloatFmt`/`FloatScan` are the numeric hooks implemented over `raven-libc-dtoa` (C42) by
//! the shell. `donor-map/04` 3.1 D-S1, 3.8 and 4.3 are the contract.

use crate::types::{DigitsOut, FloatBits, FloatKind, FmtErr, StoreSize};

/// One unit of the output character domain.
///
/// Only `u8` (narrow `printf`) and `u32` (wide `wprintf`) implement this trait; `wchar_t` and
/// `wint_t` are both 32-bit unsigned on every supported target (`donor-map/04` D-S2).
pub trait CharUnit: Copy + Eq + Default {
    /// True for the wide engine.
    const WIDE: bool;

    /// The code unit for an ASCII byte.
    fn from_ascii(b: u8) -> Self;

    /// The numeric value of the unit.
    fn to_u32(self) -> u32;

    /// The unit for a numeric value, truncated to the unit width.
    fn from_u32(v: u32) -> Self;
}

impl CharUnit for u8 {
    const WIDE: bool = false;

    fn from_ascii(b: u8) -> Self {
        b
    }

    fn to_u32(self) -> u32 {
        u32::from(self)
    }

    fn from_u32(v: u32) -> Self {
        v as u8
    }
}

impl CharUnit for u32 {
    const WIDE: bool = true;

    fn from_ascii(b: u8) -> Self {
        u32::from(b)
    }

    fn to_u32(self) -> u32 {
        self
    }

    fn from_u32(v: u32) -> Self {
        v
    }
}

/// The sink refused a write; the engine returns [`FmtErr::Sink`].
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct SinkError;

/// An output destination of the formatting engine.
///
/// The glue holds the `FILE` lock for the whole call; the engine writes each conversion
/// through this trait and never flushes (`donor-map/04` T20, A-stdio-12).
pub trait Sink<C: CharUnit> {
    /// Write all of `s`.
    ///
    /// # Errors
    /// [`SinkError`] when the destination fails.
    fn write(&mut self, s: &[C]) -> Result<(), SinkError>;

    /// Write `n` copies of `ch`.
    ///
    /// # Errors
    /// [`SinkError`] when the destination fails.
    fn pad(&mut self, ch: C, n: usize) -> Result<(), SinkError> {
        const CHUNK: usize = 32;
        let block = [ch; CHUNK];
        let mut left = n;
        while left > 0 {
            let k = if left >= CHUNK { CHUNK } else { left };
            self.write(block.get(..k).unwrap_or(&block))?;
            left = left.wrapping_sub(k);
        }
        Ok(())
    }
}

/// The real `va_list` reader (`donor-map/04` 3.9; the per-architecture types are C59's).
///
/// Integer and pointer arguments occupy 8-byte slots whose upper halves are undefined for
/// 32-bit arguments; the engine truncates by the length modifier and never trusts them
/// (A-stdio-3).
pub trait VaReader {
    /// The next integer, pointer or `wint_t` word.
    fn next_u64(&mut self) -> u64;

    /// The bits of the next `double` argument.
    fn next_f64_bits(&mut self) -> u64;

    /// The little-endian memory image of the next `long double` argument.
    fn next_ld_bits(&mut self) -> u128;

    /// A copy of the reader at its current position (`va_copy`, A-stdio-4).
    fn clone_state(&self) -> Self
    where
        Self: Sized;
}

/// Every operation of the engine that dereferences a C pointer value.
///
/// The engine never forms a pointer; the shell implements these methods with the real
/// `malloc`, `strnlen`, `wcrtomb`, `strerror` and `%n` store (`donor-map/04` A-stdio-2).
pub trait FmtEnv: VaReader {
    /// `strnlen` semantics: the number of bytes before the first NUL, never reading past
    /// `max` bytes (T17).
    fn cstr_len(&mut self, ptr: usize, max: usize) -> usize;

    /// Copy up to `buf.len()` bytes starting at `ptr + offset`; return the number copied.
    fn read_bytes(&mut self, ptr: usize, offset: usize, buf: &mut [u8]) -> usize;

    /// The number of wide characters before the first NUL, never reading past `max`.
    fn wstr_len(&mut self, ptr: usize, max: usize) -> usize;

    /// Copy up to `buf.len()` wide characters starting at `ptr + offset * 4`.
    fn read_wchars(&mut self, ptr: usize, offset: usize, buf: &mut [u32]) -> usize;

    /// Store a `%n` count with the given width.
    fn store_count(&mut self, ptr: usize, size: StoreSize, value: i64);

    /// Write the `%m` text for `errnum` into `buf`; return its length.
    fn errno_text(&mut self, errnum: i32, buf: &mut [u8; 64]) -> usize;

    /// Store a `scanf` integer with the given width.
    fn store_int(&mut self, ptr: usize, size: StoreSize, value: u64);

    /// Store a byte string (without NUL) at `ptr`.
    fn store_chars(&mut self, ptr: usize, data: &[u8]);

    /// Store a wide string (without NUL) at `ptr`; the units are 32-bit little-endian.
    fn store_wchars(&mut self, ptr: usize, data: &[u32]);

    /// Store the bits of a `scanf` floating-point value at `ptr`.
    fn store_float_bits(&mut self, ptr: usize, kind: FloatKind, bits: u128);

    /// Allocate and store the result of `%m`, including its NUL terminator.
    ///
    /// For `wide`, `bytes` is the little-endian image of the `u32` string. Return `false`
    /// when the allocation failed.
    fn alloc_store(&mut self, dest_ptr: usize, bytes: &[u8], wide: bool) -> bool;
}

/// The decimal digit hook; the shell implements it over `raven-libc-dtoa` (C42).
pub trait FloatFmt {
    /// Decimal digits for `%e`/`%g` (`mode` 2) or `%f` (`mode` 3).
    ///
    /// # Errors
    /// [`FmtErr::NoMem`] when the caller's buffer is too small or the dtoa arena is
    /// exhausted; the dummy hooks return [`FmtErr::Invalid`] or
    /// [`FmtErr::FloatUnavailable`].
    fn dec_digits(
        &self,
        v: FloatBits,
        mode: u8,
        ndigits: i32,
        out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr>;

    /// Hexadecimal digits for `%a`; `ndigits < 0` means exact.
    ///
    /// # Errors
    /// As [`FloatFmt::dec_digits`].
    fn hex_digits(
        &self,
        v: FloatBits,
        upper: bool,
        ndigits: i32,
        out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr>;
}

/// The `scanf` floating-point hook; the token is already validated by the lexer.
pub trait FloatScan {
    /// Convert a validated token.
    ///
    /// # Errors
    /// [`FmtErr::Invalid`] when the hook refuses the conversion; the scan engine turns that
    /// into a matching failure.
    fn parse(&self, token: &[u8], kind: FloatKind) -> Result<FloatBits, FmtErr>;
}

/// The hook of the integer-only profile: consumes the argument bits and fails with
/// [`FmtErr::Invalid`] (`donor-map/04` 3.8).
#[derive(Clone, Copy, Default)]
pub struct NoFloat;

impl FloatFmt for NoFloat {
    fn dec_digits(
        &self,
        _v: FloatBits,
        _mode: u8,
        _ndigits: i32,
        _out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr> {
        Err(FmtErr::Invalid)
    }

    fn hex_digits(
        &self,
        _v: FloatBits,
        _upper: bool,
        _ndigits: i32,
        _out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr> {
        Err(FmtErr::Invalid)
    }
}

/// The weak-hook dummy of the tiny profile: the engine prints the literal `*float*` for the
/// conversion and continues (`donor-map/16` R-13).
#[derive(Clone, Copy, Default)]
pub struct StubFloat;

impl FloatFmt for StubFloat {
    fn dec_digits(
        &self,
        _v: FloatBits,
        _mode: u8,
        _ndigits: i32,
        _out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr> {
        Err(FmtErr::FloatUnavailable)
    }

    fn hex_digits(
        &self,
        _v: FloatBits,
        _upper: bool,
        _ndigits: i32,
        _out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr> {
        Err(FmtErr::FloatUnavailable)
    }
}

/// The `scanf` dummy: a floating conversion is a matching failure.
#[derive(Clone, Copy, Default)]
pub struct NoScanFloat;

impl FloatScan for NoScanFloat {
    fn parse(&self, _token: &[u8], _kind: FloatKind) -> Result<FloatBits, FmtErr> {
        Err(FmtErr::Invalid)
    }
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
#[path = "traits_ut.rs"]
mod tests;
