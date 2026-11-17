// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `wcstod`, `wcstof`, `wcstol` and `wcstoul`.

use core::ffi::{c_char, c_double, c_float, c_void};

use super::wcsto_support::{encode, units_for_bytes};

/// `wcstod`.
///
/// # Safety
/// `input` is NUL-terminated; `endptr` is NULL or writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstod(input: *const u32, endptr: *mut *mut u32) -> c_double {
    if input.is_null() {
        return 0.0;
    }
    let bytes = encode(input);
    let mut converted: *mut c_char = core::ptr::null_mut();
    // SAFETY: the temporary UTF-8 buffer is NUL-terminated.
    let value = unsafe {
        crate::stdlib::atof::strtod(bytes.as_ptr().cast(), &mut converted)
    };
    if !endptr.is_null() {
        let offset = (converted as usize).saturating_sub(bytes.as_ptr() as usize);
        // SAFETY: the caller grants a writable end pointer.
        unsafe { *endptr = input.add(units_for_bytes(input, offset)).cast_mut() };
    }
    value
}

/// `wcstof`.
///
/// # Safety
/// As `wcstod`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstof(input: *const u32, endptr: *mut *mut u32) -> c_float {
    if input.is_null() {
        return 0.0;
    }
    let bytes = encode(input);
    let mut converted: *mut c_char = core::ptr::null_mut();
    // SAFETY: the temporary UTF-8 buffer is NUL-terminated.
    let value = unsafe {
        crate::stdlib::strtof::strtof(bytes.as_ptr().cast(), &mut converted)
    };
    if !endptr.is_null() {
        let offset = (converted as usize).saturating_sub(bytes.as_ptr() as usize);
        // SAFETY: the caller grants a writable end pointer.
        unsafe { *endptr = input.add(units_for_bytes(input, offset)).cast_mut() };
    }
    value
}

/// `wcstol`.
///
/// # Safety
/// As `wcstod`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstol(input: *const u32, endptr: *mut *mut u32, base: core::ffi::c_int) -> i64 {
    if input.is_null() {
        return 0;
    }
    let bytes = encode(input);
    let mut converted: *mut c_char = core::ptr::null_mut();
    // SAFETY: the temporary UTF-8 buffer is NUL-terminated.
    let value = unsafe {
        crate::stdlib::strtol::strtol(bytes.as_ptr().cast(), &mut converted, base)
    };
    if !endptr.is_null() {
        let offset = (converted as usize).saturating_sub(bytes.as_ptr() as usize);
        // SAFETY: the caller grants a writable end pointer.
        unsafe { *endptr = input.add(units_for_bytes(input, offset)).cast_mut() };
    }
    value
}

/// `wcstoul`.
///
/// # Safety
/// As `wcstod`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wcstoul(input: *const u32, endptr: *mut *mut u32, base: core::ffi::c_int) -> u64 {
    if input.is_null() {
        return 0;
    }
    let bytes = encode(input);
    let mut converted: *mut c_char = core::ptr::null_mut();
    // SAFETY: the temporary UTF-8 buffer is NUL-terminated.
    let value = unsafe {
        crate::stdlib::strtoul::strtoul(bytes.as_ptr().cast(), &mut converted, base)
    };
    if !endptr.is_null() {
        let offset = (converted as usize).saturating_sub(bytes.as_ptr() as usize);
        // SAFETY: the caller grants a writable end pointer.
        unsafe { *endptr = input.add(units_for_bytes(input, offset)).cast_mut() };
    }
    value
}

#[allow(dead_code)]
fn _keep(_: *mut c_void) {}
