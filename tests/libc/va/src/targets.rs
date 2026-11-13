// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The test-defined `v*` targets: the real engine through `CEnv`, the recorder targets of the
//! table and the aggregate recorder.
//!
//! This module is test code: it may use `std`, `f64` and `unsafe`. It is included by the
//! integration tests, never by the library build.

#![allow(dead_code)] // included by several test binaries, each using a subset

use std::sync::Mutex;
use std::vec::Vec;

use raven_libc_dtoa::{
    C_NUMERIC, DigitMode, DtoaCtx, Kind, LongDouble, MAX_DIGITS_F64, MAX_DIGITS_LD, Rounding, dtoa,
    hdtoa, hldtoa, ldtoa, strtod, strtof, strtold,
};
use raven_libc_fmt::{
    DEC_BUF_F64, DEC_BUF_LD, DigitsOut, FloatBits, FloatClass, FloatFmt, FloatKind, FloatScan,
    FmtEnv, FmtErr, FmtOptions, ScanOptions, ScanResult, ScanSource, StoreSize, VaReader,
    glue::vsnprintf_core, vscan,
};

use raven_libc_va_test::va::{
    CEnv, NativeReader, VaListAapcs64, VaListSysV, VaReaderExt, VaSaveAapcs64, VaSaveRiscv64,
    VaSaveSysV,
};

const _: () = assert!(DEC_BUF_F64 == MAX_DIGITS_F64);
const _: () = assert!(DEC_BUF_LD == MAX_DIGITS_LD);

// The save-area types are referenced by the model-facing helpers so that a layout change is
// caught here as well as in the module tests.
const _: () = assert!(core::mem::size_of::<VaSaveSysV>() == 176);
const _: () = assert!(core::mem::size_of::<VaSaveAapcs64>() == 208);
const _: () = assert!(core::mem::size_of::<VaListAapcs64>() == 32);
const _: () = assert!(core::mem::size_of::<VaSaveRiscv64>() == 80);

/// The `va_list` parameter type of a `v*` target on the architecture the tests run on.
#[cfg(target_arch = "x86_64")]
pub type Ap = *mut VaListSysV;
/// The `va_list` parameter type of a `v*` target on the architecture the tests run on.
#[cfg(target_arch = "aarch64")]
pub type Ap = *mut VaListAapcs64;
/// The `va_list` parameter type of a `v*` target on the architecture the tests run on.
#[cfg(target_arch = "riscv64")]
pub type Ap = *mut u8;

/// Construct the native reader for one `va_list` argument.
///
/// # Safety
///
/// `ap` must be the live `va_list` the stub passed to the target.
#[must_use]
pub unsafe fn native_reader(ap: Ap) -> NativeReader {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: the caller passes the stub's `va_list`.
        unsafe { raven_libc_va_test::va::SysvReader::from_ptr(ap) }
    }
    #[cfg(target_arch = "aarch64")]
    {
        // SAFETY: the caller passes the stub's `va_list`.
        unsafe { raven_libc_va_test::va::Aapcs64Reader::from_ptr(ap) }
    }
    #[cfg(target_arch = "riscv64")]
    {
        // SAFETY: the caller passes the stub's cursor.
        unsafe { raven_libc_va_test::va::RiscvReader::from_cursor(ap) }
    }
}

fn class(kind: Kind) -> FloatClass {
    match kind {
        Kind::Zero => FloatClass::Zero,
        Kind::Finite => FloatClass::Finite,
        Kind::Inf => FloatClass::Inf,
        Kind::Nan => FloatClass::Nan,
    }
}

fn lo(ld: &LongDouble) -> u128 {
    u128::from_le_bytes(ld.0)
}

/// The strong float hook over `raven-libc-dtoa` (the same adapter C43 uses in the oracle).
#[derive(Clone, Copy, Default)]
pub struct RealFloat;

impl FloatFmt for RealFloat {
    fn dec_digits(
        &self,
        v: FloatBits,
        mode: u8,
        ndigits: i32,
        out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr> {
        let mut ctx = DtoaCtx::new();
        let dmode = if mode == 2 {
            DigitMode::SigDigits
        } else {
            DigitMode::Decimals
        };
        let r = Rounding::Nearest;
        let d = match v.kind {
            FloatKind::F32 => dtoa(
                &mut ctx,
                f64::from(f32::from_bits(v.bits as u32)),
                dmode,
                ndigits,
                r,
                out,
            ),
            FloatKind::F64 => dtoa(
                &mut ctx,
                f64::from_bits(v.bits as u64),
                dmode,
                ndigits,
                r,
                out,
            ),
            FloatKind::LongDouble => ldtoa(
                &mut ctx,
                LongDouble(v.bits.to_le_bytes()),
                dmode,
                ndigits,
                r,
                out,
            ),
        }
        .map_err(|e| match e {
            raven_libc_dtoa::DtoaError::BufferTooSmall { .. } => FmtErr::NoMem,
            raven_libc_dtoa::DtoaError::NoMemory => FmtErr::NoMem,
        })?;
        Ok(DigitsOut {
            len: d.len,
            decpt: d.decpt,
            neg: d.negative,
            class: class(d.kind),
        })
    }

    fn hex_digits(
        &self,
        v: FloatBits,
        upper: bool,
        ndigits: i32,
        out: &mut [u8],
    ) -> Result<DigitsOut, FmtErr> {
        let r = Rounding::Nearest;
        let d = match v.kind {
            FloatKind::F32 => hdtoa(
                f64::from(f32::from_bits(v.bits as u32)),
                upper,
                ndigits,
                r,
                out,
            ),
            FloatKind::F64 => hdtoa(f64::from_bits(v.bits as u64), upper, ndigits, r, out),
            FloatKind::LongDouble => {
                hldtoa(LongDouble(v.bits.to_le_bytes()), upper, ndigits, r, out)
            }
        }
        .map_err(|e| match e {
            raven_libc_dtoa::DtoaError::BufferTooSmall { .. } => FmtErr::NoMem,
            raven_libc_dtoa::DtoaError::NoMemory => FmtErr::NoMem,
        })?;
        Ok(DigitsOut {
            len: d.len,
            decpt: d.exp2.saturating_add(1),
            neg: d.negative,
            class: class(d.kind),
        })
    }
}

/// The strong scan hook over C42.
#[derive(Clone, Copy, Default)]
pub struct RealScan;

impl FloatScan for RealScan {
    fn parse(&self, token: &[u8], kind: FloatKind) -> Result<FloatBits, FmtErr> {
        let mut ctx = DtoaCtx::new();
        let locale = C_NUMERIC;
        let rounding = Rounding::Nearest;
        let bits = match kind {
            FloatKind::F32 => {
                let parsed =
                    strtof(&mut ctx, token, &locale, rounding).map_err(|_| FmtErr::NoMem)?;
                if parsed.consumed == 0 {
                    return Err(FmtErr::Invalid);
                }
                u128::from(parsed.value.to_bits())
            }
            FloatKind::F64 => {
                let parsed =
                    strtod(&mut ctx, token, &locale, rounding).map_err(|_| FmtErr::NoMem)?;
                if parsed.consumed == 0 {
                    return Err(FmtErr::Invalid);
                }
                u128::from(parsed.value.to_bits())
            }
            FloatKind::LongDouble => {
                let parsed =
                    strtold(&mut ctx, token, &locale, rounding).map_err(|_| FmtErr::NoMem)?;
                if parsed.consumed == 0 {
                    return Err(FmtErr::Invalid);
                }
                lo(&parsed.value)
            }
        };
        Ok(FloatBits { kind, bits })
    }
}

/// A `%m` text service for the tests.
fn fake_errno_text(errnum: i32, buf: &mut [u8; 64]) -> usize {
    let text = if errnum == 0 {
        "no error".as_bytes()
    } else {
        "test error".as_bytes()
    };
    let len = text.len().min(buf.len());
    buf[..len].copy_from_slice(&text[..len]);
    len
}

/// A `%m` allocator for the tests: a leaked block; the tests are short lived.
fn fake_alloc(size: usize) -> *mut u8 {
    let mut buffer = vec![0u8; size];
    let pointer = buffer.as_mut_ptr();
    core::mem::forget(buffer);
    pointer
}

/// Read a NUL-terminated C string.
///
/// # Safety
///
/// `ptr` must be a valid NUL-terminated string for the duration of the call.
unsafe fn cstr(ptr: *const u8) -> &'static [u8] {
    if ptr.is_null() {
        return b"";
    }
    let mut len = 0usize;
    // SAFETY: the caller guarantees a NUL-terminated string at `ptr`.
    while unsafe { *ptr.add(len) } != 0 {
        len += 1;
    }
    // SAFETY: the string lives in the caller's frame for the whole call; the lifetime is
    // narrowed to this call by the surrounding functions.
    unsafe { core::slice::from_raw_parts(ptr, len) }
}

/// A `std::slice`-backed [`ScanSource`] for `sscanf`.
pub struct SliceSource<'a> {
    data: &'a [u8],
    position: usize,
    pushed: Vec<u8>,
}

impl<'a> SliceSource<'a> {
    /// Scan `data`.
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self {
            data,
            position: 0,
            pushed: Vec::new(),
        }
    }
}

impl ScanSource<u8> for SliceSource<'_> {
    fn getc(&mut self) -> Option<u8> {
        if let Some(byte) = self.pushed.pop() {
            return Some(byte);
        }
        let byte = *self.data.get(self.position)?;
        self.position += 1;
        Some(byte)
    }

    fn ungetc(&mut self, c: u8) {
        self.pushed.push(c);
    }
}

/// `t_vsnprintf`: the real `vsnprintf` path, the test's stand-in for C60's target.
pub extern "C" fn vsnprintf(buf: *mut u8, n: usize, fmt: *const u8, ap: Ap) -> i32 {
    // SAFETY: the stub passes its own live `va_list`; the format is the caller's string.
    let mut env = unsafe { CEnv::new(native_reader(ap), fake_errno_text, fake_alloc) };
    // SAFETY: the format pointer is the C caller's NUL-terminated string.
    let format = unsafe { cstr(fmt) };
    let out: &mut [u8] = if n == 0 || buf.is_null() {
        &mut []
    } else {
        // SAFETY: the caller passed `n` writable bytes.
        unsafe { core::slice::from_raw_parts_mut(buf, n) }
    };
    let opts = FmtOptions::default();
    vsnprintf_core(out, n, format, &mut env, &opts, &RealFloat).unwrap_or(-1)
}

/// `t_vsscanf`: the real `vsscanf` path.
pub extern "C" fn vsscanf(input: *const u8, fmt: *const u8, ap: Ap) -> i32 {
    // SAFETY: the stub passes its own live `va_list`; the format is the caller's string.
    let mut env = unsafe { CEnv::new(native_reader(ap), fake_errno_text, fake_alloc) };
    // SAFETY: the format pointer is the C caller's NUL-terminated string.
    let format = unsafe { cstr(fmt) };
    // SAFETY: the input pointer is the C caller's NUL-terminated string.
    let text = unsafe { cstr(input) };
    let mut source = SliceSource::new(text);
    let opts = ScanOptions::default();
    match vscan(&mut source, format, &mut env, &opts, &RealScan) {
        Ok(ScanResult {
            assigned,
            return_eof,
            ..
        }) => {
            if return_eof {
                -1
            } else {
                assigned
            }
        }
        Err(_) => -1,
    }
}

// ---------------------------------------------------------------------------------------
// Recording targets
// ---------------------------------------------------------------------------------------

/// One recorded target invocation.
#[derive(Clone, Debug, Default)]
pub struct Record {
    /// The number of fixed leading arguments the row declares.
    pub named: usize,
    /// The named argument words the target received.
    pub words: Vec<u64>,
    /// The first integer slots the reader returned.
    pub values: Vec<u64>,
    /// The first `double` bits the reader returned.
    pub doubles: Vec<u64>,
}

static RECORDS: Mutex<Vec<Record>> = Mutex::new(Vec::new());

/// Drop every record.
pub fn reset_records() {
    let mut records = RECORDS.lock().unwrap_or_else(|poison| poison.into_inner());
    records.clear();
}

/// Take every record.
pub fn take_records() -> Vec<Record> {
    let mut records = RECORDS.lock().unwrap_or_else(|poison| poison.into_inner());
    core::mem::take(&mut *records)
}

/// Record `count` integer slots and `floats` double slots from `ap`.
pub fn record(named: usize, words: &[u64], ap: Ap, count: usize) {
    // SAFETY: the stub passes its own live `va_list`.
    let mut reader = unsafe { native_reader(ap) };
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(reader.next_u64());
    }
    let record = Record {
        named,
        words: words.to_vec(),
        values,
        doubles: Vec::new(),
    };
    let mut records = RECORDS.lock().unwrap_or_else(|poison| poison.into_inner());
    records.push(record);
}

/// The recorder target of one table row.
///
/// The signature must match the row's `named` count: the trampoline passes exactly `named`
/// words and then `&va_list` in the next integer register.
#[macro_export]
macro_rules! recorder_named {
    (name: $name:ident, named: 1) => {
        /// Recorder target of one variadic export.
        pub extern "C" fn $name(a0: usize, ap: $crate::targets::Ap) -> i32 {
            $crate::targets::record(1, &[a0 as u64], ap, 3);
            0x5A5A
        }
    };
    (name: $name:ident, named: 2) => {
        /// Recorder target of one variadic export.
        pub extern "C" fn $name(a0: usize, a1: usize, ap: $crate::targets::Ap) -> i32 {
            $crate::targets::record(2, &[a0 as u64, a1 as u64], ap, 3);
            0x5A5A
        }
    };
    (name: $name:ident, named: 3) => {
        /// Recorder target of one variadic export.
        pub extern "C" fn $name(a0: usize, a1: usize, a2: usize, ap: $crate::targets::Ap) -> i32 {
            $crate::targets::record(3, &[a0 as u64, a1 as u64, a2 as u64], ap, 3);
            0x5A5A
        }
    };
    (name: $name:ident, named: 4) => {
        /// Recorder target of one variadic export.
        pub extern "C" fn $name(
            a0: usize,
            a1: usize,
            a2: usize,
            a3: usize,
            ap: $crate::targets::Ap,
        ) -> i32 {
            $crate::targets::record(4, &[a0 as u64, a1 as u64, a2 as u64, a3 as u64], ap, 3);
            0x5A5A
        }
    };
}

/// The aggregate bytes the `t_rec` target read, per selector.
static AGGREGATES: Mutex<Vec<(usize, Vec<u8>)>> = Mutex::new(Vec::new());

/// Drop the recorded aggregates.
pub fn reset_aggregates() {
    let mut aggregates = AGGREGATES
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    aggregates.clear();
}

/// Take the recorded aggregates.
pub fn take_aggregates() -> Vec<(usize, Vec<u8>)> {
    let mut aggregates = AGGREGATES
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    core::mem::take(&mut *aggregates)
}

/// `t_rec`: reads one integer-class aggregate of the size the selector names.
pub extern "C" fn rec_agg(selector: usize, ap: Ap) -> i32 {
    // SAFETY: the stub passes its own live `va_list`.
    let mut env = unsafe { CEnv::new(native_reader(ap), fake_errno_text, fake_alloc) };
    let mut out = [0u8; 32];
    if !env.reader.next_aggregate_int(selector, 8, &mut out) {
        return -1;
    }
    let mut aggregates = AGGREGATES
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    aggregates.push((selector, out[..selector].to_vec()));
    0
}

// `t_rec`: one extra instance of the macro, outside the table (the struct-by-value layer).
raven_libc_va_test::raven_variadic!(
    prefix: "t_",
    export: "rec",
    target: crate::targets::rec_agg,
    named: 1
);

// ---------------------------------------------------------------------------------------
// Small helpers the tests use
// ---------------------------------------------------------------------------------------

/// The value `t_vsnprintf` returns for a successful call of a known length (test helper).
#[must_use]
pub fn engine_len(format: &[u8], words: &[u64]) -> i32 {
    let mut out = [0u8; 256];
    let capacity = out.len();
    let reader = NullReader::new(words);
    let mut env = CEnv::new(reader, fake_errno_text, fake_alloc);
    let opts = FmtOptions::default();
    vsnprintf_core(&mut out, capacity, format, &mut env, &opts, &RealFloat).unwrap_or(-1)
}

/// A reader over a word list, for `engine_len`.
pub struct NullReader {
    words: Vec<u64>,
    position: usize,
}

impl NullReader {
    /// Read `words` as integer slots and then zeroes.
    #[must_use]
    pub fn new(words: &[u64]) -> Self {
        Self {
            words: words.to_vec(),
            position: 0,
        }
    }
}

impl VaReader for NullReader {
    fn next_u64(&mut self) -> u64 {
        let value = self.words.get(self.position).copied().unwrap_or(0);
        self.position += 1;
        value
    }

    fn next_f64_bits(&mut self) -> u64 {
        self.next_u64()
    }

    fn next_ld_bits(&mut self) -> u128 {
        u128::from(self.next_u64())
    }

    fn clone_state(&self) -> Self {
        Self {
            words: self.words.clone(),
            position: self.position,
        }
    }
}

impl VaReaderExt for NullReader {
    fn next_aggregate_int(&mut self, size: usize, _align: usize, out: &mut [u8; 32]) -> bool {
        let value = self.next_u64().to_le_bytes();
        let target = out.get_mut(..size.min(8));
        if let Some(target) = target {
            target.copy_from_slice(&value[..size.min(8)]);
            return true;
        }
        false
    }
}

impl FmtEnv for NullReader {
    fn cstr_len(&mut self, _ptr: usize, _max: usize) -> usize {
        0
    }

    fn read_bytes(&mut self, _ptr: usize, _offset: usize, _buf: &mut [u8]) -> usize {
        0
    }

    fn wstr_len(&mut self, _ptr: usize, _max: usize) -> usize {
        0
    }

    fn read_wchars(&mut self, _ptr: usize, _offset: usize, _buf: &mut [u32]) -> usize {
        0
    }

    fn store_count(&mut self, _ptr: usize, _size: StoreSize, _value: i64) {}

    fn errno_text(&mut self, errnum: i32, buf: &mut [u8; 64]) -> usize {
        fake_errno_text(errnum, buf)
    }

    fn store_int(&mut self, _ptr: usize, _size: StoreSize, _value: u64) {}

    fn store_chars(&mut self, _ptr: usize, _data: &[u8]) {}

    fn store_wchars(&mut self, _ptr: usize, _data: &[u32]) {}

    fn store_float_bits(&mut self, _ptr: usize, _kind: FloatKind, _bits: u128) {}

    fn alloc_store(&mut self, _dest_ptr: usize, _bytes: &[u8], _wide: bool) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "targets_ut.rs"]
mod tests;
