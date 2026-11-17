// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `strfromd` and `strfromf`: the C23 float-to-string conversions.
//!
//! The conversion specifier is the caller's `<stdio.h>` conversion string, so the printf engine
//! and its float hook do the actual formatting. The reader below presents the single value as the
//! only variable argument.

use core::ffi::{c_char, c_double, c_float, c_int};

use raven_libc_fmt::VaReader;
use raven_libc_fmt::glue::vsnprintf_core;

use crate::stdio::vprintf::{FloatHook, alloc, errno_text, options};
use crate::va::{CEnv, VaReaderExt};

#[derive(Clone, Copy)]
struct OneFloat {
    bits: u64,
}

impl VaReader for OneFloat {
    fn next_u64(&mut self) -> u64 {
        self.bits
    }

    fn next_f64_bits(&mut self) -> u64 {
        self.bits
    }

    fn next_ld_bits(&mut self) -> u128 {
        0
    }

    fn clone_state(&self) -> Self {
        *self
    }
}

impl VaReaderExt for OneFloat {
    fn next_aggregate_int(&mut self, _size: usize, _align: usize, _out: &mut [u8; 32]) -> bool {
        false
    }
}

fn format_float(s: *mut c_char, n: usize, format: *const c_char, value: f64) -> c_int {
    // SAFETY: the caller grants a writable `n`-byte buffer.
    let out = unsafe { crate::ffi::cstr::slice_mut(s, n) };
    // SAFETY: the format is the caller's NUL-terminated conversion string.
    let format = unsafe { crate::ffi::cstr::slice(format) };
    let mut env = CEnv::new(
        OneFloat {
            bits: value.to_bits(),
        },
        errno_text,
        alloc,
    );
    let options = options();
    match vsnprintf_core(out, n, format, &mut env, &options, &FloatHook) {
        Ok(value) => value,
        Err(error) => crate::ffi::map_fmt::apply(error),
    }
}

/// `strfromd`: format one `double` into a bounded buffer.
///
/// # Safety
/// `s` has `n` writable bytes; `format` is NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strfromd(
    s: *mut c_char,
    n: usize,
    format: *const c_char,
    value: c_double,
) -> c_int {
    format_float(s, n, format, value)
}

/// `strfromf`: format one `float` into a bounded buffer.
///
/// # Safety
/// As `strfromd`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strfromf(
    s: *mut c_char,
    n: usize,
    format: *const c_char,
    value: c_float,
) -> c_int {
    format_float(s, n, format, f64::from(value))
}
