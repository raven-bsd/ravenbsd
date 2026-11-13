// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `support_fmt`: the real float adapter over `raven-libc-dtoa`, the oracle
//! argument environment and the shape dispatch that calls the C variadic
//! `snprintf`/`sscanf` family.
//!
//! Stable Rust can call a C variadic function only with a compile-time
//! argument list. `call_printf_words` passes up to four pointer-sized words;
//! `call_printf_dbl` handles one `double` at a known position and words
//! elsewhere. The differential matrix keeps at most one floating conversion
//! per format, so this covers every generated cell; the K03 alternating
//! int/double shapes are hand-written calls in the test (the ticket's 4.7
//! 340-arm table collapses to this documented shape set).

use std::vec::Vec;

use raven_libc_dtoa::{
    C_NUMERIC, DigitMode, DtoaCtx, Kind, LongDouble, MAX_DIGITS_F64, MAX_DIGITS_LD, Rounding, dtoa,
    hdtoa, hldtoa, ldtoa, strtod, strtof, strtold,
};
use raven_libc_fmt::{
    DEC_BUF_F64, DEC_BUF_LD, DigitsOut, FloatBits, FloatClass, FloatFmt, FloatKind, FloatScan,
    FmtEnv, FmtErr, StoreSize, VaReader,
};

const _: () = assert!(DEC_BUF_F64 == MAX_DIGITS_F64);
const _: () = assert!(DEC_BUF_LD == MAX_DIGITS_LD);

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

/// The strong float hook over C42 (the same adapter the shell will use).
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
            // C42 publishes `exp2` as the exponent printed after `p`; the engine's
            // `DigitsOut.decpt` is the donor's hdtoa `decpt`, which the engine prints as
            // `decpt - 1`.
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
        let loc = C_NUMERIC;
        let r = Rounding::Nearest;
        let bits = match kind {
            FloatKind::F32 => {
                let p = strtof(&mut ctx, token, &loc, r).map_err(|_| FmtErr::NoMem)?;
                if p.consumed == 0 {
                    return Err(FmtErr::Invalid);
                }
                u128::from(p.value.to_bits())
            }
            FloatKind::F64 => {
                let p = strtod(&mut ctx, token, &loc, r).map_err(|_| FmtErr::NoMem)?;
                if p.consumed == 0 {
                    return Err(FmtErr::Invalid);
                }
                u128::from(p.value.to_bits())
            }
            FloatKind::LongDouble => {
                let p = strtold(&mut ctx, token, &loc, r).map_err(|_| FmtErr::NoMem)?;
                if p.consumed == 0 {
                    return Err(FmtErr::Invalid);
                }
                lo(&p.value)
            }
        };
        Ok(FloatBits { kind, bits })
    }
}

/// A slice-backed argument reader and pointer arena for the oracle side.
pub struct OracleVa {
    /// Integer words in call order.
    pub words: Vec<u64>,
    /// Double bits in call order.
    pub dbls: Vec<u64>,
    /// Long-double images in call order.
    pub lds: Vec<u128>,
    /// Pointer arena; offsets are pointer values; byte 0 is the NULL sentinel.
    pub arena: Vec<u8>,
    /// Fail `alloc_store`.
    pub fail_alloc: bool,
}

impl OracleVa {
    /// An empty environment.
    #[must_use]
    pub fn new() -> Self {
        Self {
            words: Vec::new(),
            dbls: Vec::new(),
            lds: Vec::new(),
            arena: vec![0],
            fail_alloc: false,
        }
    }

    /// Append an integer word.
    pub fn word(&mut self, v: u64) -> &mut Self {
        self.words.push(v);
        self
    }

    /// Append a `double`.
    pub fn dbl(&mut self, v: f64) -> &mut Self {
        self.dbls.push(v.to_bits());
        self
    }

    /// Append a C string; returns the arena offset.
    pub fn cstr(&mut self, s: &[u8]) -> usize {
        let off = self.arena.len();
        self.arena.extend_from_slice(s);
        self.arena.push(0);
        off
    }

    /// Append a wide string; returns the arena offset.
    pub fn wstr(&mut self, s: &[u32]) -> usize {
        let off = self.arena.len();
        for &u in s {
            self.arena.extend_from_slice(&u.to_le_bytes());
        }
        self.arena.extend_from_slice(&0u32.to_le_bytes());
        off
    }

    /// Reserve a zero cell and return its offset.
    pub fn cell(&mut self, n: usize) -> usize {
        let off = self.arena.len();
        self.arena.resize(off.saturating_add(n), 0);
        off
    }

    /// Read a little-endian value of `n` bytes.
    #[must_use]
    pub fn read(&self, ptr: usize, n: usize) -> u64 {
        let mut out = 0u64;
        for i in 0..n {
            out |=
                u64::from(self.arena.get(ptr.saturating_add(i)).copied().unwrap_or(0)) << (8 * i);
        }
        out
    }

    /// Read a stored pointer.
    #[must_use]
    pub fn read_ptr(&self, ptr: usize) -> usize {
        self.read(ptr, 8) as usize
    }

    fn put(&mut self, ptr: usize, bytes: &[u8]) {
        if ptr == 0 {
            return;
        }
        for (i, &b) in bytes.iter().enumerate() {
            if let Some(slot) = self.arena.get_mut(ptr.saturating_add(i)) {
                *slot = b;
            }
        }
    }
}

impl Default for OracleVa {
    fn default() -> Self {
        Self::new()
    }
}

impl VaReader for OracleVa {
    fn next_u64(&mut self) -> u64 {
        if self.words.is_empty() {
            0
        } else {
            self.words.remove(0)
        }
    }

    fn next_f64_bits(&mut self) -> u64 {
        if self.dbls.is_empty() {
            0
        } else {
            self.dbls.remove(0)
        }
    }

    fn next_ld_bits(&mut self) -> u128 {
        if self.lds.is_empty() {
            0
        } else {
            self.lds.remove(0)
        }
    }

    fn clone_state(&self) -> Self {
        Self {
            words: self.words.clone(),
            dbls: self.dbls.clone(),
            lds: self.lds.clone(),
            arena: self.arena.clone(),
            fail_alloc: self.fail_alloc,
        }
    }
}

impl FmtEnv for OracleVa {
    fn cstr_len(&mut self, ptr: usize, max: usize) -> usize {
        let mut n = 0usize;
        while n < max {
            let Some(&b) = self.arena.get(ptr.saturating_add(n)) else {
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
            n = n.saturating_add(1);
        }
        n
    }

    fn wstr_len(&mut self, ptr: usize, max: usize) -> usize {
        let mut n = 0usize;
        while n < max {
            if self.read(ptr.saturating_add(n.saturating_mul(4)), 4) == 0 {
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
            if let Some(slot) = buf.get_mut(n) {
                *slot = self.read(at, 4) as u32;
            }
            n = n.saturating_add(1);
        }
        n
    }

    fn store_count(&mut self, ptr: usize, size: StoreSize, value: i64) {
        self.put(ptr, &(value as u64).to_le_bytes()[..size.bytes()]);
    }

    fn errno_text(&mut self, errnum: i32, buf: &mut [u8; 64]) -> usize {
        let text = std::format!("errno {errnum}");
        let n = std::cmp::min(text.len(), buf.len());
        if let Some(dst) = buf.get_mut(..n) {
            dst.copy_from_slice(&text.as_bytes()[..n]);
        }
        n
    }

    fn store_int(&mut self, ptr: usize, size: StoreSize, value: u64) {
        self.put(ptr, &value.to_le_bytes()[..size.bytes()]);
    }

    fn store_chars(&mut self, ptr: usize, data: &[u8]) {
        self.put(ptr, data);
    }

    fn store_wchars(&mut self, ptr: usize, data: &[u32]) {
        for (i, &u) in data.iter().enumerate() {
            self.put(ptr.saturating_add(i.saturating_mul(4)), &u.to_le_bytes());
        }
    }

    fn store_float_bits(&mut self, ptr: usize, kind: FloatKind, bits: u128) {
        let n = match kind {
            FloatKind::F32 => 4,
            FloatKind::F64 => 8,
            FloatKind::LongDouble => 16,
        };
        self.put(ptr, &bits.to_le_bytes()[..n]);
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
        self.put(dest_ptr, &(off as u64).to_le_bytes());
        true
    }
}

/// Format with the Rust engine; returns the output bytes.
pub fn format_u8(
    fmt: &[u8],
    args: &mut OracleVa,
    opts: &raven_libc_fmt::FmtOptions<'_>,
) -> Result<Vec<u8>, FmtErr> {
    use raven_libc_fmt::{Sink, SinkError};

    struct VecSink {
        data: Vec<u8>,
    }

    impl Sink<u8> for VecSink {
        fn write(&mut self, s: &[u8]) -> Result<(), SinkError> {
            self.data.extend_from_slice(s);
            Ok(())
        }
        fn pad(&mut self, ch: u8, n: usize) -> Result<(), SinkError> {
            self.data.extend(std::iter::repeat_n(ch, n));
            Ok(())
        }
    }

    let mut sink = VecSink { data: Vec::new() };
    raven_libc_fmt::vformat(&mut sink, fmt, args, opts, &RealFloat)?;
    Ok(sink.data)
}

/// Format with the Rust wide engine; returns the output units.
pub fn format_u32(
    fmt: &[u32],
    args: &mut OracleVa,
    opts: &raven_libc_fmt::FmtOptions<'_>,
) -> Result<Vec<u32>, FmtErr> {
    use raven_libc_fmt::{Sink, SinkError};

    struct VecSink {
        data: Vec<u32>,
    }

    impl Sink<u32> for VecSink {
        fn write(&mut self, s: &[u32]) -> Result<(), SinkError> {
            self.data.extend_from_slice(s);
            Ok(())
        }
        fn pad(&mut self, ch: u32, n: usize) -> Result<(), SinkError> {
            self.data.extend(std::iter::repeat_n(ch, n));
            Ok(())
        }
    }

    let mut sink = VecSink { data: Vec::new() };
    raven_libc_fmt::vformat(&mut sink, fmt, args, opts, &RealFloat)?;
    Ok(sink.data)
}

/// One argument of a C printf call: a pointer-sized word or a `double`.
#[derive(Clone, Copy)]
pub struct Arg {
    /// The word value (pointer bits included).
    pub i: i64,
    /// The double value.
    pub d: f64,
}

impl Arg {
    /// A word argument.
    #[must_use]
    pub fn word(v: i64) -> Self {
        Self { i: v, d: 0.0 }
    }

    /// A pointer argument.
    #[must_use]
    pub fn ptr(p: *mut core::ffi::c_void) -> Self {
        Self {
            i: p as i64,
            d: 0.0,
        }
    }

    /// A double argument.
    #[must_use]
    pub fn dbl(v: f64) -> Self {
        Self { i: 0, d: v }
    }
}

/// Call a C `snprintf` with up to four pointer-sized words.
///
/// # Safety
/// `buf`, `cap` and `fmt` must describe a writable buffer and a valid NUL
/// terminated format; the argument words must match the format.
pub unsafe fn call_printf_words(
    buf: *mut core::ffi::c_char,
    cap: usize,
    fmt: *const core::ffi::c_char,
    args: &[Arg],
    f: unsafe extern "C" fn(
        *mut core::ffi::c_char,
        usize,
        *const core::ffi::c_char,
        ...
    ) -> core::ffi::c_int,
) -> core::ffi::c_int {
    // SAFETY: the caller's `# Safety` contract covers the buffer, format and
    // argument list.
    unsafe {
        match args.len() {
            0 => f(buf, cap, fmt),
            1 => f(buf, cap, fmt, args[0].i),
            2 => f(buf, cap, fmt, args[0].i, args[1].i),
            3 => f(buf, cap, fmt, args[0].i, args[1].i, args[2].i),
            _ => f(buf, cap, fmt, args[0].i, args[1].i, args[2].i, args[3].i),
        }
    }
}

/// Call a C wide `swprintf` with up to four pointer-sized words.
///
/// # Safety
/// As [`call_printf_words`].
pub unsafe fn call_swprintf_words(
    buf: *mut i32,
    cap: usize,
    fmt: *const i32,
    args: &[Arg],
    f: unsafe extern "C" fn(*mut i32, usize, *const i32, ...) -> core::ffi::c_int,
) -> core::ffi::c_int {
    // SAFETY: the caller's `# Safety` contract covers the buffer, format and
    // argument list.
    unsafe {
        match args.len() {
            0 => f(buf, cap, fmt),
            1 => f(buf, cap, fmt, args[0].i),
            2 => f(buf, cap, fmt, args[0].i, args[1].i),
            3 => f(buf, cap, fmt, args[0].i, args[1].i, args[2].i),
            _ => f(buf, cap, fmt, args[0].i, args[1].i, args[2].i, args[3].i),
        }
    }
}

/// Call a C `snprintf` with one `double` at `dpos` and words elsewhere.
///
/// # Safety
/// As [`call_printf_words`]; `dpos < args.len()`.
pub unsafe fn call_printf_dbl(
    buf: *mut core::ffi::c_char,
    cap: usize,
    fmt: *const core::ffi::c_char,
    args: &[Arg],
    dpos: usize,
    f: unsafe extern "C" fn(
        *mut core::ffi::c_char,
        usize,
        *const core::ffi::c_char,
        ...
    ) -> core::ffi::c_int,
) -> core::ffi::c_int {
    // SAFETY: the caller's `# Safety` contract covers the buffer, format and
    // argument list.
    unsafe {
        match (args.len(), dpos) {
            (1, 0) => f(buf, cap, fmt, args[0].d),
            (2, 0) => f(buf, cap, fmt, args[0].d, args[1].i),
            (2, 1) => f(buf, cap, fmt, args[0].i, args[1].d),
            (3, 0) => f(buf, cap, fmt, args[0].d, args[1].i, args[2].i),
            (3, 1) => f(buf, cap, fmt, args[0].i, args[1].d, args[2].i),
            (3, 2) => f(buf, cap, fmt, args[0].i, args[1].i, args[2].d),
            (4, 0) => f(buf, cap, fmt, args[0].d, args[1].i, args[2].i, args[3].i),
            (4, 1) => f(buf, cap, fmt, args[0].i, args[1].d, args[2].i, args[3].i),
            (4, 2) => f(buf, cap, fmt, args[0].i, args[1].i, args[2].d, args[3].i),
            (4, 3) => f(buf, cap, fmt, args[0].i, args[1].i, args[2].i, args[3].d),
            _ => -1,
        }
    }
}

/// Call a C `sscanf` with up to four pointers.
///
/// # Safety
/// The pointers must be valid destinations for the format.
pub unsafe fn call_sscanf(
    input: *const core::ffi::c_char,
    fmt: *const core::ffi::c_char,
    args: &[*mut core::ffi::c_void],
    f: unsafe extern "C" fn(
        *const core::ffi::c_char,
        *const core::ffi::c_char,
        ...
    ) -> core::ffi::c_int,
) -> core::ffi::c_int {
    // SAFETY: the caller's `# Safety` contract covers the buffer, format and
    // argument list.
    unsafe {
        match args.len() {
            0 => f(input, fmt),
            1 => f(input, fmt, args[0]),
            2 => f(input, fmt, args[0], args[1]),
            3 => f(input, fmt, args[0], args[1], args[2]),
            _ => f(input, fmt, args[0], args[1], args[2], args[3]),
        }
    }
}

/// Call a C wide `swscanf` with up to four pointers.
///
/// # Safety
/// As [`call_sscanf`]; the wide input and format are `wchar_t` arrays.
pub unsafe fn call_swscanf(
    input: *const i32,
    fmt: *const i32,
    args: &[*mut core::ffi::c_void],
    f: unsafe extern "C" fn(*const i32, *const i32, ...) -> core::ffi::c_int,
) -> core::ffi::c_int {
    // SAFETY: the caller's `# Safety` contract covers the buffer, format and
    // argument list.
    unsafe {
        match args.len() {
            0 => f(input, fmt),
            1 => f(input, fmt, args[0]),
            2 => f(input, fmt, args[0], args[1]),
            3 => f(input, fmt, args[0], args[1], args[2]),
            _ => f(input, fmt, args[0], args[1], args[2], args[3]),
        }
    }
}
