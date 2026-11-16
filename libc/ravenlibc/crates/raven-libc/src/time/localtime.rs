// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#[cfg(not(target_arch = "x86_64"))]
use core::ffi::c_double;
use core::ffi::c_longlong;
use core::ptr;

use raven_libc_abi::structs::Tm as AbiTm;

use super::{convert, fill_abi_tm, local_tz, read_abi_tm, static_tm};

/// `localtime`.
///
/// # Safety
/// `timep` is readable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn localtime(timep: *const c_longlong) -> *mut AbiTm {
    if timep.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: the C contract says `timep` is readable.
    let seconds = unsafe { *timep };
    let state = local_tz();
    match convert::localtime_r(&state, seconds) {
        Ok(tm) => static_tm(&tm),
        Err(error) => crate::ffi::ret::null(error),
    }
}

/// `mktime`.
///
/// # Safety
/// `tm` is readable and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mktime(tm: *mut AbiTm) -> c_longlong {
    if tm.is_null() {
        return -1;
    }
    // SAFETY: the C contract says `tm` is readable and writable.
    let source = unsafe { &mut *tm };
    let mut value = read_abi_tm(source);
    let state = local_tz();
    match raven_libc_time::tz::mktime::mktime(&state, &mut value) {
        Ok(seconds) => {
            fill_abi_tm(source, &value);
            seconds
        }
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}

/// `timegm`.
///
/// # Safety
/// `tm` is readable and writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timegm(tm: *mut AbiTm) -> c_longlong {
    if tm.is_null() {
        return -1;
    }
    // SAFETY: the C contract says `tm` is readable and writable.
    let source = unsafe { &mut *tm };
    let mut value = read_abi_tm(source);
    match raven_libc_time::tz::mktime::timegm(&mut value) {
        Ok(seconds) => {
            fill_abi_tm(source, &value);
            seconds
        }
        Err(error) => {
            crate::ffi::errno::set(error);
            -1
        }
    }
}

/// `difftime` on the non-x86_64 targets.
#[cfg(not(target_arch = "x86_64"))]
#[unsafe(no_mangle)]
pub extern "C" fn difftime(t1: c_longlong, t0: c_longlong) -> c_double {
    raven_libc_time::difftime::difftime(t1, t0)
}

/// `difftime` on x86_64.
///
/// The hosted C caller passes and receives `double` in the SysV SSE registers
/// since S22 enabled the hard-float time case, while the libc target itself is
/// the soft-float profile. The assembly entry converts both operands and
/// subtracts them in `xmm0`, so the signed difference cannot wrap.
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .section .text
    .global difftime
    .type difftime, @function
difftime:
    cvtsi2sd xmm0, rdi
    cvtsi2sd xmm1, rsi
    subsd xmm0, xmm1
    ret
    .size difftime, . - difftime
"#
);
