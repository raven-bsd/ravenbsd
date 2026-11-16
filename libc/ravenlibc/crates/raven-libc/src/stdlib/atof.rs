// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `atof(3)` and `strtod(3)`.
//!
//! The conversion engine is `raven-libc-dtoa`; this file only owns the C string and `endptr`
//! contract and the `ERANGE`/`ENOMEM` errno mapping.

use core::ffi::{c_char, c_double, c_int};

use raven_libc_dtoa::{C_NUMERIC, DtoaCtx, RangeFlags, Rounding, strtod as dtoa_strtod};

/// `atof`: `strtod(s, NULL)`.
///
/// # Safety
/// `s` is a NUL-terminated string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn atof(s: *const c_char) -> c_double {
    // SAFETY: as `strtod`.
    unsafe { strtod(s, core::ptr::null_mut()) }
}

/// `strtod`: parse a decimal/hex/inf/nan string and write the end pointer.
///
/// # Safety
/// `s` is NUL-terminated; `endptr` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtod(s: *const c_char, endptr: *mut *mut c_char) -> c_double {
    // SAFETY: the caller's string contract.
    let bytes = unsafe { crate::ffi::cstr::slice(s) };
    let mut context = DtoaCtx::new();
    match dtoa_strtod(&mut context, bytes, &C_NUMERIC, Rounding::Nearest) {
        Ok(parsed) => {
            if !endptr.is_null() {
                // SAFETY: the caller grants a writable pointer slot; `consumed <= bytes.len()`.
                unsafe { *endptr = s.add(parsed.consumed).cast_mut() };
            }
            if parsed.range.contains(RangeFlags::OVERFLOW)
                || parsed.range.contains(RangeFlags::UNDERFLOW)
            {
                crate::ffi::errno::set_raw(c_int::from(raven_libc_abi::errno::ERANGE));
            }
            parsed.value
        }
        Err(_) => {
            crate::ffi::errno::set_raw(c_int::from(raven_libc_abi::errno::ENOMEM));
            if !endptr.is_null() {
                // SAFETY: as above; no conversion consumed input.
                unsafe { *endptr = s.cast_mut() };
            }
            0.0
        }
    }
}
