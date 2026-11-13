// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Test support shared by the `_ut.rs` unit tests and the integration tests.
//!
//! `SliceVa` is the slice-backed `VaReader`/`FmtEnv`: arguments are words in a `Vec`, C
//! strings and wide strings live in a byte arena whose offsets are the pointer values, and
//! every store is recorded. The arena starts with one sentinel byte so offset 0 is the NULL
//! pointer. `VecSink` collects output and can be told to fail after `k` writes;
//! `RecordingSource` is a `ScanSource` that counts `getc`/`ungetc` calls.

#![allow(dead_code)] // the support module is shared by several test binaries

use std::string::String;
use std::vec::Vec;

use raven_libc_fmt::{
    CharUnit, DigitsOut, FloatBits, FloatClass, FloatFmt, FloatKind, FloatScan, FmtEnv, FmtErr,
    ScanSource, Sink, SinkError, StoreSize, VaReader,
};

/// One variadic argument slot.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum ArgWord {
    /// An integer or pointer word (upper half may be garbage).
    Int(u64),
    /// The bits of a `double`.
    Dbl(u64),
    /// The raw image of a `long double`.
    Ld(u128),
    /// A pointer value (arena offset).
    Ptr(usize),
}

/// One recorded store.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum StoreEvent {
    /// `%n` or `%d` store: `(ptr, size, value)`.
    Int(usize, StoreSize, u64),
    /// A byte-string store.
    Chars(usize, usize),
    /// A wide-string store.
    WChars(usize, usize),
    /// A float store: `(ptr, kind, bits)`.
    Float(usize, FloatKind, u128),
    /// An allocation store: `(dest_ptr, bytes, wide)`.
    Alloc(usize, usize, bool),
}

/// A slice-backed reader and pointer environment.
pub struct SliceVa {
    /// The argument words.
    pub words: Vec<ArgWord>,
    /// The next word index.
    pub pos: usize,
    /// The pointer arena; offsets are pointer values.
    pub arena: Vec<u8>,
    /// Recorded stores.
    pub stores: Vec<StoreEvent>,
    /// Every `next_*` call in order (`0` int, `1` double, `2` long double).
    pub reads: Vec<u8>,
    /// The highest arena byte index read by a string operation (T17).
    pub max_read: usize,
    /// Fail `alloc_store` when true.
    pub fail_alloc: bool,
    /// The `%m` text value.
    pub errno_value: i32,
}

impl SliceVa {
    /// A reader with an empty argument list and the NULL sentinel at offset 0.
    #[must_use]
    pub fn new() -> Self {
        Self {
            words: Vec::new(),
            pos: 0,
            arena: vec![0],
            stores: Vec::new(),
            reads: Vec::new(),
            max_read: 0,
            fail_alloc: false,
            errno_value: 0,
        }
    }

    /// Append one integer word (upper half garbage, as A-stdio-3 warns).
    pub fn int(&mut self, v: u64) -> &mut Self {
        self.words.push(ArgWord::Int(v));
        self
    }

    /// Append one integer word with garbage in the upper 32 bits.
    pub fn int_garbage(&mut self, v: u32) -> &mut Self {
        self.words
            .push(ArgWord::Int(0xdead_beef_0000_0000 | u64::from(v)));
        self
    }

    /// Append a `double` argument by bits.
    pub fn dbl_bits(&mut self, bits: u64) -> &mut Self {
        self.words.push(ArgWord::Dbl(bits));
        self
    }

    /// Append a `double` argument.
    pub fn dbl(&mut self, v: f64) -> &mut Self {
        self.dbl_bits(v.to_bits())
    }

    /// Append a `long double` image.
    pub fn ld_bits(&mut self, bits: u128) -> &mut Self {
        self.words.push(ArgWord::Ld(bits));
        self
    }

    /// Append a pointer argument.
    pub fn ptr(&mut self, offset: usize) -> &mut Self {
        self.words.push(ArgWord::Ptr(offset));
        self
    }

    /// Append a C string and return its arena offset.
    pub fn cstr(&mut self, s: &str) -> usize {
        let off = self.arena.len();
        self.arena.extend_from_slice(s.as_bytes());
        self.arena.push(0);
        off
    }

    /// Append a wide string and return its arena offset.
    pub fn wstr(&mut self, s: &[u32]) -> usize {
        let off = self.arena.len();
        for &wc in s {
            self.arena.extend_from_slice(&wc.to_le_bytes());
        }
        self.arena.extend_from_slice(&0u32.to_le_bytes());
        off
    }

    /// Reserve `n` zero bytes and return the arena offset.
    pub fn cell(&mut self, n: usize) -> usize {
        let off = self.arena.len();
        self.arena.resize(off.saturating_add(n), 0);
        off
    }

    /// Read a little-endian unsigned value of `n` bytes from the arena.
    #[must_use]
    pub fn read_cell(&self, ptr: usize, n: usize) -> u64 {
        let mut out = 0u64;
        for i in 0..n {
            let b = self.arena.get(ptr.saturating_add(i)).copied().unwrap_or(0);
            out |= u64::from(b) << (8 * i);
        }
        out
    }

    /// Read a stored pointer value.
    #[must_use]
    pub fn read_ptr(&self, ptr: usize) -> usize {
        self.read_cell(ptr, 8) as usize
    }

    fn take(&mut self, kind: u8) -> u64 {
        self.reads.push(kind);
        let Some(word) = self.words.get(self.pos).copied() else {
            return 0;
        };
        self.pos = self.pos.saturating_add(1);
        match word {
            ArgWord::Int(v) | ArgWord::Dbl(v) => v,
            ArgWord::Ld(v) => v as u64,
            ArgWord::Ptr(v) => v as u64,
        }
    }

    fn store_bytes(&mut self, ptr: usize, bytes: &[u8]) {
        if ptr == 0 {
            return;
        }
        for (i, &b) in bytes.iter().enumerate() {
            if let Some(slot) = self.arena.get_mut(ptr.saturating_add(i)) {
                *slot = b;
            }
        }
    }

    fn store_u32s(&mut self, ptr: usize, units: &[u32]) {
        if ptr == 0 {
            return;
        }
        for (i, &u) in units.iter().enumerate() {
            self.store_bytes(ptr.saturating_add(i.saturating_mul(4)), &u.to_le_bytes());
        }
    }
}

impl Default for SliceVa {
    fn default() -> Self {
        Self::new()
    }
}

impl VaReader for SliceVa {
    fn next_u64(&mut self) -> u64 {
        self.take(0)
    }

    fn next_f64_bits(&mut self) -> u64 {
        self.take(1)
    }

    fn next_ld_bits(&mut self) -> u128 {
        self.reads.push(2);
        let Some(word) = self.words.get(self.pos).copied() else {
            return 0;
        };
        self.pos = self.pos.saturating_add(1);
        match word {
            ArgWord::Ld(v) => v,
            ArgWord::Int(v) | ArgWord::Dbl(v) => u128::from(v),
            ArgWord::Ptr(v) => v as u128,
        }
    }

    fn clone_state(&self) -> Self {
        Self {
            words: self.words.clone(),
            pos: self.pos,
            arena: self.arena.clone(),
            stores: self.stores.clone(),
            reads: self.reads.clone(),
            max_read: self.max_read,
            fail_alloc: self.fail_alloc,
            errno_value: self.errno_value,
        }
    }
}

impl FmtEnv for SliceVa {
    fn cstr_len(&mut self, ptr: usize, max: usize) -> usize {
        if max == 0 {
            return 0;
        }
        let mut n = 0usize;
        while n < max {
            let at = ptr.saturating_add(n);
            self.max_read = std::cmp::max(self.max_read, at);
            let Some(&b) = self.arena.get(at) else {
                break;
            };
            if b == 0 {
                break;
            }
            n = n.saturating_add(1);
        }
        n
    }

    fn read_bytes(&mut self, ptr: usize, offset: usize, buf: &mut [u8]) -> usize {
        let start = ptr.saturating_add(offset);
        let mut n = 0usize;
        while n < buf.len() {
            let Some(&b) = self.arena.get(start.saturating_add(n)) else {
                break;
            };
            if let Some(slot) = buf.get_mut(n) {
                *slot = b;
            }
            self.max_read = std::cmp::max(self.max_read, start.saturating_add(n));
            n = n.saturating_add(1);
        }
        n
    }

    fn wstr_len(&mut self, ptr: usize, max: usize) -> usize {
        let mut n = 0usize;
        while n < max {
            let at = ptr.saturating_add(n.saturating_mul(4));
            self.max_read = std::cmp::max(self.max_read, at.saturating_add(3));
            let v = self.read_cell(at, 4) as u32;
            if v == 0 {
                break;
            }
            n = n.saturating_add(1);
        }
        n
    }

    fn read_wchars(&mut self, ptr: usize, offset: usize, buf: &mut [u32]) -> usize {
        let start = ptr.saturating_add(offset.saturating_mul(4));
        let mut n = 0usize;
        while n < buf.len() {
            let at = start.saturating_add(n.saturating_mul(4));
            if at >= self.arena.len() {
                break;
            }
            let v = self.read_cell(at, 4) as u32;
            if let Some(slot) = buf.get_mut(n) {
                *slot = v;
            }
            self.max_read = std::cmp::max(self.max_read, at.saturating_add(3));
            n = n.saturating_add(1);
        }
        n
    }

    fn store_count(&mut self, ptr: usize, size: StoreSize, value: i64) {
        if ptr == 0 {
            return;
        }
        self.stores.push(StoreEvent::Int(ptr, size, value as u64));
        let bytes = (value as u64).to_le_bytes();
        self.store_bytes(ptr, bytes.get(..size.bytes()).unwrap_or(&[]));
    }

    fn errno_text(&mut self, errnum: i32, buf: &mut [u8; 64]) -> usize {
        let text = format!("errno {errnum}");
        let bytes = text.as_bytes();
        let n = std::cmp::min(bytes.len(), buf.len());
        if let Some(dst) = buf.get_mut(..n) {
            dst.copy_from_slice(bytes.get(..n).unwrap_or(&[]));
        }
        n
    }

    fn store_int(&mut self, ptr: usize, size: StoreSize, value: u64) {
        if ptr == 0 {
            return;
        }
        self.stores.push(StoreEvent::Int(ptr, size, value));
        self.store_bytes(ptr, value.to_le_bytes().get(..size.bytes()).unwrap_or(&[]));
    }

    fn store_chars(&mut self, ptr: usize, data: &[u8]) {
        if ptr == 0 {
            return;
        }
        self.stores.push(StoreEvent::Chars(ptr, data.len()));
        self.store_bytes(ptr, data);
    }

    fn store_wchars(&mut self, ptr: usize, data: &[u32]) {
        if ptr == 0 {
            return;
        }
        self.stores.push(StoreEvent::WChars(ptr, data.len()));
        self.store_u32s(ptr, data);
    }

    fn store_float_bits(&mut self, ptr: usize, kind: FloatKind, bits: u128) {
        if ptr == 0 {
            return;
        }
        self.stores.push(StoreEvent::Float(ptr, kind, bits));
        let n = match kind {
            FloatKind::F32 => 4,
            FloatKind::F64 => 8,
            FloatKind::LongDouble => 16,
        };
        self.store_bytes(ptr, bits.to_le_bytes().get(..n).unwrap_or(&[]));
    }

    fn alloc_store(&mut self, dest_ptr: usize, bytes: &[u8], wide: bool) -> bool {
        if self.fail_alloc {
            return false;
        }
        let off = self.arena.len();
        self.arena.extend_from_slice(bytes);
        let unit = if wide { 4 } else { 1 };
        self.arena
            .resize(off.saturating_add(bytes.len()).saturating_add(unit), 0);
        self.stores
            .push(StoreEvent::Alloc(dest_ptr, bytes.len(), wide));
        self.store_bytes(dest_ptr, (off as u64).to_le_bytes().get(..8).unwrap_or(&[]));
        true
    }
}

/// The recording environment is [`SliceVa`] itself; every store is recorded in `stores`.
pub type RecordingEnv = SliceVa;

/// A sink that collects output units and can fail after `k` writes.
pub struct VecSink<C> {
    /// Collected units.
    pub data: Vec<C>,
    /// Number of successful `write`/`pad` calls.
    pub writes: usize,
    /// Fail once `writes` reaches this value.
    pub fail_after: Option<usize>,
}

impl<C> VecSink<C> {
    /// An empty sink.
    #[must_use]
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            writes: 0,
            fail_after: None,
        }
    }

    /// Fail after `k` write calls.
    #[must_use]
    pub fn failing(k: usize) -> Self {
        Self {
            data: Vec::new(),
            writes: 0,
            fail_after: Some(k),
        }
    }

    /// The collected units.
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len()
    }
}

impl<C> Default for VecSink<C> {
    fn default() -> Self {
        Self::new()
    }
}

impl<C: CharUnit> Sink<C> for VecSink<C> {
    fn write(&mut self, s: &[C]) -> Result<(), SinkError> {
        if let Some(k) = self.fail_after {
            if self.writes >= k {
                return Err(SinkError);
            }
        }
        self.writes = self.writes.saturating_add(1);
        self.data.extend_from_slice(s);
        Ok(())
    }

    fn pad(&mut self, ch: C, n: usize) -> Result<(), SinkError> {
        if n == 0 {
            return Ok(());
        }
        if let Some(k) = self.fail_after {
            if self.writes >= k {
                return Err(SinkError);
            }
        }
        self.writes = self.writes.saturating_add(1);
        for _ in 0..n {
            self.data.push(ch);
        }
        Ok(())
    }
}

/// A `ScanSource` over a slice that counts calls and pushback.
pub struct RecordingSource<C> {
    /// The input units.
    pub input: Vec<C>,
    /// The read position.
    pub pos: usize,
    /// The pushback stack.
    pub ungets: Vec<C>,
    /// `getc` calls.
    pub getc_calls: usize,
    /// `ungetc` calls.
    pub ungetc_calls: usize,
    /// The largest number of outstanding pushbacks seen.
    pub max_outstanding: usize,
}

impl<C> RecordingSource<C> {
    /// A source over `input`.
    #[must_use]
    pub fn new(input: Vec<C>) -> Self {
        Self {
            input,
            pos: 0,
            ungets: Vec::new(),
            getc_calls: 0,
            ungetc_calls: 0,
            max_outstanding: 0,
        }
    }
}

impl<C: CharUnit> ScanSource<C> for RecordingSource<C> {
    fn getc(&mut self) -> Option<C> {
        self.getc_calls = self.getc_calls.saturating_add(1);
        if let Some(c) = self.ungets.pop() {
            return Some(c);
        }
        let c = self.input.get(self.pos).copied();
        if c.is_some() {
            self.pos = self.pos.saturating_add(1);
        }
        c
    }

    fn ungetc(&mut self, c: C) {
        self.ungetc_calls = self.ungetc_calls.saturating_add(1);
        self.ungets.push(c);
        self.max_outstanding = std::cmp::max(self.max_outstanding, self.ungets.len());
    }
}

/// A float hook whose decimal digits come from `std` formatting and whose hex digits come
/// from the IEEE fields (test-only; the real adapter over `raven-libc-dtoa` lives in the
/// oracle crate).
#[derive(Clone, Copy, Default)]
pub struct FakeFloat;

fn class_of(v: f64) -> FloatClass {
    if v.is_nan() {
        FloatClass::Nan
    } else if v.is_infinite() {
        FloatClass::Inf
    } else if v == 0.0 {
        FloatClass::Zero
    } else {
        FloatClass::Finite
    }
}

fn trim_zeroes(digits: &mut Vec<u8>) {
    while digits.len() > 1 && digits.last() == Some(&b'0') {
        digits.pop();
    }
}

impl FloatFmt for FakeFloat {
    fn dec_digits(
        &self,
        v: FloatBits,
        mode: u8,
        ndigits: i32,
        out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr> {
        let value = match v.kind {
            FloatKind::F32 => f32::from_bits(v.bits as u32) as f64,
            FloatKind::F64 => f64::from_bits(v.bits as u64),
            FloatKind::LongDouble => {
                // The test fake cannot interpret x87/binary128; return a fixed pattern so
                // the layout rules can still be exercised.
                let digits = b"15";
                let n = std::cmp::min(digits.len(), out.len());
                if let Some(dst) = out.get_mut(..n) {
                    dst.copy_from_slice(digits.get(..n).unwrap_or(&[]));
                }
                return Ok(DigitsOut {
                    len: n,
                    decpt: 1,
                    neg: (v.bits >> 79) & 1 == 1,
                    class: FloatClass::Finite,
                });
            }
        };
        let class = class_of(value);
        if matches!(class, FloatClass::Inf | FloatClass::Nan) {
            return Ok(DigitsOut {
                len: 0,
                decpt: 0,
                neg: value.is_sign_negative(),
                class,
            });
        }
        let neg = value.is_sign_negative();
        let mut text = if mode == 2 {
            let p = if ndigits <= 0 { 1 } else { ndigits as usize };
            format!("{:.*e}", p.saturating_sub(1), value.abs())
        } else {
            let p = if ndigits < 0 { 0 } else { ndigits as usize };
            format!("{:.*}", p, value.abs())
        };
        let mut digits: Vec<u8> = Vec::new();
        let mut decpt: i32;
        if mode == 2 {
            let (mant, exp) = match text.split_once('e') {
                Some((m, e)) => (m.to_string(), e.parse::<i32>().unwrap_or(0)),
                None => (text.clone(), 0),
            };
            for b in mant.bytes() {
                if b != b'.' {
                    digits.push(b);
                }
            }
            decpt = exp.saturating_add(1);
        } else {
            let (int, frac) = match text.split_once('.') {
                Some((i, f)) => (i.to_string(), f.to_string()),
                None => (text.clone(), String::new()),
            };
            let int_len = int.len();
            let mut all = int;
            all.push_str(&frac);
            let lead = all.bytes().take_while(|&b| b == b'0').count();
            digits.extend(all.bytes().skip(lead));
            decpt = (int_len as i32).saturating_sub(lead as i32);
            if digits.is_empty() {
                digits.push(b'0');
                decpt = 1;
            }
        }
        trim_zeroes(&mut digits);
        let n = std::cmp::min(digits.len(), out.len());
        if let Some(dst) = out.get_mut(..n) {
            dst.copy_from_slice(digits.get(..n).unwrap_or(&[]));
        }
        text.clear();
        Ok(DigitsOut {
            len: n,
            decpt: if class == FloatClass::Zero { 1 } else { decpt },
            neg,
            class,
        })
    }

    fn hex_digits(
        &self,
        v: FloatBits,
        upper: bool,
        ndigits: i32,
        out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr> {
        let bits = match v.kind {
            FloatKind::F32 => u64::from(v.bits as u32),
            FloatKind::F64 => v.bits as u64,
            FloatKind::LongDouble => {
                if let Some(slot) = out.first_mut() {
                    *slot = b'1';
                }
                return Ok(DigitsOut {
                    len: 1,
                    decpt: 1,
                    neg: (v.bits >> 79) & 1 == 1,
                    class: FloatClass::Finite,
                });
            }
        };
        let neg = (bits >> 63) & 1 == 1;
        let exp = ((bits >> 52) & 0x7ff) as i32;
        let frac = bits & ((1u64 << 52) - 1);
        if exp == 0x7ff {
            return Ok(DigitsOut {
                len: 0,
                decpt: 0,
                neg,
                class: if frac != 0 {
                    FloatClass::Nan
                } else {
                    FloatClass::Inf
                },
            });
        }
        if exp == 0 && frac == 0 {
            if let Some(slot) = out.first_mut() {
                *slot = b'0';
            }
            return Ok(DigitsOut {
                len: 1,
                decpt: 1,
                neg,
                class: FloatClass::Zero,
            });
        }
        let mut lead = if exp == 0 { 0u8 } else { 1u8 };
        let mut decpt = if exp == 0 { -1022 } else { exp - 1023 };
        let mut significand = if exp == 0 { frac } else { (1u64 << 52) | frac };
        let mut shift = 48i32;
        if exp == 0 {
            while significand & (1u64 << 52) == 0 && significand != 0 {
                significand <<= 1;
                decpt = decpt.saturating_sub(1);
                lead = 0;
            }
            shift = 48;
        }
        let xdigs: &[u8; 16] = if upper {
            b"0123456789ABCDEF"
        } else {
            b"0123456789abcdef"
        };
        let mut digits: Vec<u8> = Vec::new();
        digits.push(xdigs.get(lead as usize).copied().unwrap_or(b'0'));
        loop {
            let nib = ((significand >> shift) & 0xf) as usize;
            digits.push(xdigs.get(nib).copied().unwrap_or(b'0'));
            if shift == 0 {
                break;
            }
            shift = shift.saturating_sub(4);
        }
        trim_zeroes(&mut digits);
        let want = if ndigits < 0 {
            digits.len()
        } else {
            std::cmp::max(1, ndigits as usize)
        };
        let n = std::cmp::min(want, digits.len());
        let n = std::cmp::min(n, out.len());
        if let Some(dst) = out.get_mut(..n) {
            dst.copy_from_slice(digits.get(..n).unwrap_or(&[]));
        }
        Ok(DigitsOut {
            len: n,
            decpt: decpt.saturating_add(1),
            neg,
            class: FloatClass::Finite,
        })
    }
}

/// A scan hook that parses decimal tokens with `std`.
#[derive(Clone, Copy, Default)]
pub struct FakeScanFloat;

impl FloatScan for FakeScanFloat {
    fn parse(&self, token: &[u8], kind: FloatKind) -> Result<FloatBits, FmtErr> {
        let text = std::str::from_utf8(token).map_err(|_| FmtErr::Invalid)?;
        let lower = text.to_ascii_lowercase();
        let bits = match kind {
            FloatKind::F32 => {
                let v: f32 = lower.parse().map_err(|_| FmtErr::Invalid)?;
                u128::from(v.to_bits())
            }
            FloatKind::F64 => {
                let v: f64 = lower.parse().map_err(|_| FmtErr::Invalid)?;
                u128::from(v.to_bits())
            }
            FloatKind::LongDouble => {
                let v: f64 = lower.parse().map_err(|_| FmtErr::Invalid)?;
                u128::from(v.to_bits())
            }
        };
        Ok(FloatBits { kind, bits })
    }
}
