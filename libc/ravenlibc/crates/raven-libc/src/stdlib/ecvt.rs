// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `ecvt_r(3)` and `fcvt_r(3)`: C digits through `raven-libc-dtoa`, then the C layout from
//! `raven-libc-stdlib`.

use core::ffi::{c_char, c_int};

use raven_libc_abi::errno::{EINVAL, Errno};
use raven_libc_dtoa::{DigitMode, DtoaCtx, Kind, MAX_DIGITS_F64, Rounding, dtoa};
use raven_libc_stdlib::conv::{ecvt_digits, fcvt_digits};

/// The longest digit string a `double` conversion can need.
const DIGIT_CAPACITY: i32 = MAX_DIGITS_F64 as i32;

/// The substituted text for a non-finite value, as the C library prints it.
fn special(value: f64) -> Option<&'static [u8]> {
    if value.is_nan() {
        Some(b"nan")
    } else if value.is_infinite() {
        Some(b"inf")
    } else {
        None
    }
}

/// Write the caller's `int *decpt` / `int *sign` slots when they are non-NULL.
///
/// # Safety
/// Each non-NULL pointer is writable for one `c_int`.
unsafe fn store(
    decpt: *mut c_int,
    sign: *mut c_int,
    converted: &raven_libc_stdlib::conv::Converted,
) {
    if !decpt.is_null() {
        // SAFETY: the caller grants one writable `c_int`.
        unsafe { *decpt = converted.decpt };
    }
    if !sign.is_null() {
        // SAFETY: as above.
        unsafe { *sign = c_int::from(converted.negative) };
    }
}

/// `ecvt_r`: convert `value` to `ndigit` significant digits.
///
/// # Safety
/// `decpt`/`sign` are NULL or writable `c_int`; `buf` is writable for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ecvt_r(
    value: f64,
    ndigit: c_int,
    decpt: *mut c_int,
    sign: *mut c_int,
    buf: *mut c_char,
    len: usize,
) -> c_int {
    if buf.is_null() || len == 0 {
        crate::ffi::errno::set(Errno(EINVAL as u16));
        return -1;
    }
    // SAFETY: the caller provides `len` writable bytes.
    let out = unsafe { crate::ffi::cstr::slice_mut(buf, len) };
    let mut digits = [0u8; MAX_DIGITS_F64];
    let requested = ndigit.clamp(0, DIGIT_CAPACITY);
    let mut context = DtoaCtx::new();
    let converted = match special(value) {
        Some(text) => ecvt_digits(ndigit, value.is_sign_negative(), b"", 0, Some(text), out),
        None => match dtoa(
            &mut context,
            value,
            DigitMode::SigDigits,
            requested,
            Rounding::Nearest,
            &mut digits,
        ) {
            Ok(result) => ecvt_digits(
                ndigit,
                result.negative,
                digits.get(..result.len).unwrap_or(&[]),
                result.decpt,
                None,
                out,
            ),
            Err(_) => Err(Errno(EINVAL as u16)),
        },
    };
    match converted {
        Ok(result) => {
            // SAFETY: the caller grants the two out-parameter slots.
            unsafe { store(decpt, sign, &result) };
            0
        }
        Err(_) => {
            crate::ffi::errno::set(Errno(EINVAL as u16));
            -1
        }
    }
}

/// `fcvt_r`: convert `value` to `ndecimal` digits after the point.
///
/// # Safety
/// `decpt`/`sign` are NULL or writable `c_int`; `buf` is writable for `len` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fcvt_r(
    value: f64,
    ndecimal: c_int,
    decpt: *mut c_int,
    sign: *mut c_int,
    buf: *mut c_char,
    len: usize,
) -> c_int {
    if buf.is_null() || len == 0 {
        crate::ffi::errno::set(Errno(EINVAL as u16));
        return -1;
    }
    // SAFETY: the caller provides `len` writable bytes.
    let out = unsafe { crate::ffi::cstr::slice_mut(buf, len) };
    let mut digits = [0u8; MAX_DIGITS_F64];
    let places = ndecimal.clamp(0, DIGIT_CAPACITY);
    let mut context = DtoaCtx::new();
    let converted = match special(value) {
        Some(text) => fcvt_digits(ndecimal, value.is_sign_negative(), b"", 0, Some(text), out),
        None => match dtoa(
            &mut context,
            value,
            DigitMode::Decimals,
            places,
            Rounding::Nearest,
            &mut digits,
        ) {
            Ok(result) => {
                let mut decpt = result.decpt;
                // `dtoa` returns the digit `0` with `decpt = 1` when a non-zero value rounds
                // to zero inside the requested precision; the C layout is the empty string with
                // the point at `-ndecimal`.
                if result.kind == Kind::Finite
                    && value != 0.0
                    && result.len == 1
                    && digits.first() == Some(&b'0')
                    && result.decpt == 1
                {
                    decpt = 0i32.saturating_sub(places);
                }
                fcvt_digits(
                    ndecimal,
                    result.negative,
                    digits.get(..result.len).unwrap_or(&[]),
                    decpt,
                    None,
                    out,
                )
            }
            Err(_) => Err(Errno(EINVAL as u16)),
        },
    };
    match converted {
        Ok(result) => {
            // SAFETY: the caller grants the two out-parameter slots.
            unsafe { store(decpt, sign, &result) };
            0
        }
        Err(_) => {
            crate::ffi::errno::set(Errno(EINVAL as u16));
            -1
        }
    }
}
