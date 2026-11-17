// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `ecvt_r(3)` and `fcvt_r(3)` over the C42 digit generator.

use core::ffi::{c_char, c_double, c_int};

use raven_libc_abi::errno::{EINVAL, Errno};
use raven_libc_dtoa::{DigitMode, DtoaCtx, Kind, MAX_DIGITS_F64, Rounding, dtoa};

fn invalid() -> c_int {
    crate::ffi::ret::int(Err(Errno(EINVAL as u16)))
}

/// `ecvt_r`: `ndigit` significant digits, the decimal position and the sign.
///
/// # Safety
/// `decpt` and `sign` are writable; `buf` holds `ndigit + 1` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ecvt_r(
    value: c_double,
    ndigit: c_int,
    decpt: *mut c_int,
    sign: *mut c_int,
    buf: *mut c_char,
) -> c_int {
    if decpt.is_null() || sign.is_null() || buf.is_null() || ndigit < 0 {
        return invalid();
    }
    let digits_wanted = usize::try_from(ndigit).unwrap_or(0);
    let mut digits = [0u8; MAX_DIGITS_F64];
    let mut context = DtoaCtx::new();
    let result = match dtoa(
        &mut context,
        value,
        DigitMode::SigDigits,
        ndigit.max(1),
        Rounding::Nearest,
        &mut digits,
    ) {
        Ok(value) => value,
        Err(_) => return invalid(),
    };
    let mut position = 0usize;
    // SAFETY: the caller grants `ndigit + 1` writable bytes.
    unsafe {
        match result.kind {
            Kind::Inf => {
                core::ptr::copy_nonoverlapping(b"inf".as_ptr(), buf.cast::<u8>(), 3);
                position = 3;
                *decpt = 0;
            }
            Kind::Nan => {
                core::ptr::copy_nonoverlapping(b"nan".as_ptr(), buf.cast::<u8>(), 3);
                position = 3;
                *decpt = 0;
            }
            _ => {
                let available = result.len.min(digits_wanted);
                core::ptr::copy_nonoverlapping(digits.as_ptr(), buf.cast::<u8>(), available);
                position = available;
                while position < digits_wanted {
                    *buf.cast::<u8>().add(position) = b'0';
                    position += 1;
                }
                *decpt = result.decpt;
            }
        }
        *buf.cast::<u8>().add(position) = 0;
        *sign = c_int::from(result.negative);
    }
    0
}

/// `fcvt_r`: `ndigit` digits after the decimal point, the decimal position and the sign.
///
/// # Safety
/// `decpt` and `sign` are writable; `buf` holds `ndigit + max(decpt, 0) + 1` bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn fcvt_r(
    value: c_double,
    ndigit: c_int,
    decpt: *mut c_int,
    sign: *mut c_int,
    buf: *mut c_char,
) -> c_int {
    if decpt.is_null() || sign.is_null() || buf.is_null() || ndigit < 0 {
        return invalid();
    }
    let fraction = usize::try_from(ndigit).unwrap_or(0);
    let mut digits = [0u8; MAX_DIGITS_F64];
    let mut context = DtoaCtx::new();
    let shortest = match dtoa(
        &mut context,
        value,
        DigitMode::Shortest,
        0,
        Rounding::Nearest,
        &mut digits,
    ) {
        Ok(value) => value,
        Err(_) => return invalid(),
    };
    let significant = match shortest.kind {
        Kind::Inf | Kind::Nan => {
            // SAFETY: the caller's buffer and out-pointers.
            unsafe {
                let text = if shortest.kind == Kind::Inf { b"inf" } else { b"nan" };
                core::ptr::copy_nonoverlapping(text.as_ptr(), buf.cast::<u8>(), 3);
                *buf.cast::<u8>().add(3) = 0;
                *decpt = 0;
                *sign = c_int::from(shortest.negative);
            }
            return 0;
        }
        _ => {
            let position = shortest.decpt;
            let needed = if position > 0 {
                position.saturating_add(ndigit)
            } else {
                ndigit.saturating_add(position).max(0)
            };
            usize::try_from(needed).unwrap_or(0)
        }
    };
    let result = match dtoa(
        &mut context,
        value,
        DigitMode::SigDigits,
        c_int::try_from(significant).unwrap_or(c_int::MAX).max(1),
        Rounding::Nearest,
        &mut digits,
    ) {
        Ok(value) => value,
        Err(_) => return invalid(),
    };
    let leading = if result.decpt < 0 {
        usize::try_from(-result.decpt).unwrap_or(0)
    } else {
        0
    };
    let total = if result.decpt >= 0 {
        usize::try_from(result.decpt).unwrap_or(0).saturating_add(fraction)
    } else {
        fraction
    };
    let mut position = 0usize;
    // SAFETY: the caller grants `total + 1` writable bytes.
    unsafe {
        for _ in 0..leading.min(total) {
            *buf.cast::<u8>().add(position) = b'0';
            position += 1;
        }
        let available = result.len.min(total.saturating_sub(position));
        core::ptr::copy_nonoverlapping(
            digits.as_ptr(),
            buf.cast::<u8>().add(position),
            available,
        );
        position += available;
        while position < total {
            *buf.cast::<u8>().add(position) = b'0';
            position += 1;
        }
        *buf.cast::<u8>().add(position) = 0;
        *decpt = if result.kind == Kind::Zero { 1 } else { result.decpt };
        *sign = c_int::from(result.negative);
    }
    0
}
