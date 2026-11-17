// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `strtof(3)`: correctly rounded float parsing.

use core::ffi::{c_char, c_float, c_int};

use raven_libc_dtoa::{C_NUMERIC, DtoaCtx, RangeFlags, Rounding, strtof as dtoa_strtof};

/// `strtof`: parse a decimal/hex/inf/nan string into an `f32`.
///
/// # Safety
/// `s` is NUL-terminated; `endptr` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn strtof(s: *const c_char, endptr: *mut *mut c_char) -> c_float {
    // SAFETY: the caller's string contract.
    let bytes = unsafe { crate::ffi::cstr::slice(s) };
    let mut context = DtoaCtx::new();
    match dtoa_strtof(&mut context, bytes, &C_NUMERIC, Rounding::Nearest) {
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
