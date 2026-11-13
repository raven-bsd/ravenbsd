// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Shared helpers for the `diff_fmt_*` integration tests.
//!
//! The C variadic call is dispatched by [`support_fmt`]; this module keeps the
//! CString/wide-string arenas alive and compares the C result with the Rust
//! engine byte for byte.

#![allow(
    unsafe_code,
    clippy::unwrap_used,
    clippy::expect_used,
    dead_code,
    clippy::undocumented_unsafe_blocks,
    unused_imports
)]

use core::ffi::{c_char, c_void};
use std::ffi::CString;
use std::vec::Vec;

use raven_libc_fmt::{FmtErr, FmtOptions, ScanResult, ScanSource};
use raven_libc_oracle::support_fmt::{
    Arg, OracleVa, RealFloat, call_printf_dbl, call_printf_words, call_sscanf, call_swscanf,
    format_u8, format_u32,
};

unsafe extern "C" {
    fn setlocale(category: i32, locale: *const c_char) -> *mut c_char;
}

/// Switch the process to `C.UTF-8` so the host `wcrtomb`/`mbrtowc` are UTF-8.
pub fn set_utf8_locale() {
    let locale = CString::new("C.UTF-8").unwrap();
    unsafe {
        setlocale(6, locale.as_ptr());
    }
}

/// The C gdtoa engine has process-global free lists and a `dtoa_result`; the
/// oracle calls must be serialized.
static C_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Hold the C oracle lock for one call.
pub fn c_guard() -> std::sync::MutexGuard<'static, ()> {
    C_LOCK.lock().unwrap()
}

/// A printf comparison result.
pub struct PrintDiff {
    /// The C return value.
    pub c_ret: i32,
    /// The C bytes.
    pub c_out: Vec<u8>,
    /// The Rust result.
    pub rust: Result<Vec<u8>, FmtErr>,
}

/// Run one narrow printf case against `oracle_snprintf`.
#[cfg(feature = "oracle-fmt-freebsd")]
pub fn diff_printf(
    fmt: &[u8],
    c_args: &[Arg],
    va: &mut OracleVa,
    dpos: Option<usize>,
    opts: &FmtOptions<'_>,
) -> PrintDiff {
    set_utf8_locale();
    let _guard = c_guard();
    let mut buf = [0u8; 4096];
    let mut fmtz = fmt.to_vec();
    fmtz.push(0);
    let ret = unsafe {
        if let Some(pos) = dpos {
            call_printf_dbl(
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                fmtz.as_ptr() as *const c_char,
                c_args,
                pos,
                raven_libc_oracle::decls::fmt_freebsd::oracle_snprintf,
            )
        } else {
            call_printf_words(
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                fmtz.as_ptr() as *const c_char,
                c_args,
                raven_libc_oracle::decls::fmt_freebsd::oracle_snprintf,
            )
        }
    };
    let n = if ret < 0 {
        0
    } else {
        std::cmp::min(ret as usize, buf.len())
    };
    PrintDiff {
        c_ret: ret,
        c_out: buf[..n].to_vec(),
        rust: format_u8(fmt, va, opts),
    }
}

/// Run one narrow printf case against musl `oracle_snprintf`.
#[cfg(feature = "oracle-fmt-musl")]
pub fn diff_musl(
    fmt: &[u8],
    c_args: &[Arg],
    va: &mut OracleVa,
    dpos: Option<usize>,
    opts: &FmtOptions<'_>,
) -> PrintDiff {
    set_utf8_locale();
    let _guard = c_guard();
    let mut buf = [0u8; 4096];
    let mut fmtz = fmt.to_vec();
    fmtz.push(0);
    let ret = unsafe {
        if let Some(pos) = dpos {
            call_printf_dbl(
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                fmtz.as_ptr() as *const c_char,
                c_args,
                pos,
                raven_libc_oracle::decls::fmt_musl::oracle_snprintf,
            )
        } else {
            call_printf_words(
                buf.as_mut_ptr() as *mut c_char,
                buf.len(),
                fmtz.as_ptr() as *const c_char,
                c_args,
                raven_libc_oracle::decls::fmt_musl::oracle_snprintf,
            )
        }
    };
    let n = if ret < 0 {
        0
    } else {
        std::cmp::min(ret as usize, buf.len())
    };
    PrintDiff {
        c_ret: ret,
        c_out: buf[..n].to_vec(),
        rust: format_u8(fmt, va, opts),
    }
}

/// Keep C strings alive while their pointers are passed to the oracle.
#[derive(Default)]
pub struct CStrings {
    strings: Vec<CString>,
}

impl CStrings {
    /// A new arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a C string and return its pointer.
    pub fn add(&mut self, s: &str) -> *mut c_void {
        let c = CString::new(s.as_bytes()).unwrap();
        let p = c.as_ptr() as *mut c_void;
        self.strings.push(c);
        p
    }
}

/// Keep C wide strings alive.
#[derive(Default)]
pub struct CWide {
    strings: Vec<Vec<i32>>,
}

impl CWide {
    /// A new arena.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a wide string and return its pointer.
    pub fn add(&mut self, s: &[u32]) -> *mut c_void {
        let mut v: Vec<i32> = s.iter().map(|&u| u as i32).collect();
        v.push(0);
        let p = v.as_ptr() as *mut c_void;
        self.strings.push(v);
        p
    }
}

/// Run one narrow `sscanf` case against musl.
#[cfg(feature = "oracle-fmt-musl-scanf")]
pub fn diff_sscanf(
    input: &str,
    fmt: &str,
    c_ptrs: &[*mut c_void],
    va: &mut OracleVa,
) -> (i32, ScanResult) {
    set_utf8_locale();
    raven_libc_oracle::decls::fmt_musl_scanf::libm_probe();
    let _guard = c_guard();
    let mut inputz = input.as_bytes().to_vec();
    inputz.push(0);
    let mut fmtz = fmt.as_bytes().to_vec();
    fmtz.push(0);
    let ret = unsafe {
        call_sscanf(
            inputz.as_ptr() as *const c_char,
            fmtz.as_ptr() as *const c_char,
            c_ptrs,
            raven_libc_oracle::decls::fmt_musl_scanf::oracle_sscanf,
        )
    };
    let mut src = SliceSource {
        input: input.as_bytes(),
        pos: 0,
        ungets: Vec::new(),
    };
    let opts = raven_libc_fmt::ScanOptions::default();
    let result = raven_libc_fmt::vscan(
        &mut src,
        fmt.as_bytes(),
        va,
        &opts,
        &raven_libc_oracle::support_fmt::RealScan,
    );
    (
        ret,
        result.unwrap_or(ScanResult {
            assigned: -999,
            consumed: 0,
            return_eof: false,
        }),
    )
}

/// A minimal byte source for `vscan`.
pub struct SliceSource<'a> {
    /// The input bytes.
    pub input: &'a [u8],
    /// The read position.
    pub pos: usize,
    /// The pushback stack.
    pub ungets: Vec<u8>,
}

impl ScanSource<u8> for SliceSource<'_> {
    fn getc(&mut self) -> Option<u8> {
        if let Some(c) = self.ungets.pop() {
            return Some(c);
        }
        let c = self.input.get(self.pos).copied();
        if c.is_some() {
            self.pos = self.pos.saturating_add(1);
        }
        c
    }

    fn ungetc(&mut self, c: u8) {
        self.ungets.push(c);
    }
}

/// Run one wide `swscanf` case against musl; returns the C return value and the
/// Rust result.
#[cfg(feature = "oracle-fmt-musl-scanf")]
pub fn diff_swscanf(
    input: &[u32],
    fmt: &[u32],
    c_ptrs: &[*mut c_void],
    va: &mut OracleVa,
) -> (i32, ScanResult) {
    set_utf8_locale();
    raven_libc_oracle::decls::fmt_musl_scanf::libm_probe();
    let _guard = c_guard();
    let mut inputz: Vec<i32> = input.iter().map(|&u| u as i32).collect();
    inputz.push(0);
    let mut fmtz: Vec<i32> = fmt.iter().map(|&u| u as i32).collect();
    fmtz.push(0);
    let ret = unsafe {
        call_swscanf(
            inputz.as_ptr(),
            fmtz.as_ptr(),
            c_ptrs,
            raven_libc_oracle::decls::fmt_musl_scanf::oracle___isoc99_swscanf,
        )
    };
    let mut src = WideSource {
        input,
        pos: 0,
        ungets: Vec::new(),
    };
    let opts = raven_libc_fmt::ScanOptions::default();
    let result = raven_libc_fmt::vscan(
        &mut src,
        fmt,
        va,
        &opts,
        &raven_libc_oracle::support_fmt::RealScan,
    );
    (
        ret,
        result.unwrap_or(ScanResult {
            assigned: -999,
            consumed: 0,
            return_eof: false,
        }),
    )
}

/// A minimal wide source for `vscan`.
pub struct WideSource<'a> {
    /// The input units.
    pub input: &'a [u32],
    /// The read position.
    pub pos: usize,
    /// The pushback stack.
    pub ungets: Vec<u32>,
}

impl ScanSource<u32> for WideSource<'_> {
    fn getc(&mut self) -> Option<u32> {
        if let Some(c) = self.ungets.pop() {
            return Some(c);
        }
        let c = self.input.get(self.pos).copied();
        if c.is_some() {
            self.pos = self.pos.saturating_add(1);
        }
        c
    }

    fn ungetc(&mut self, c: u32) {
        self.ungets.push(c);
    }
}

/// Read the C output of one `%n` cell.
///
/// # Safety
/// `p` must point to at least four initialized bytes.
pub unsafe fn read_i32(p: *mut c_void) -> i32 {
    unsafe { *(p as *const i32) }
}
