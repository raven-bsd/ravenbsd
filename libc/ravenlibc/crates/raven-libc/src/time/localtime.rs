// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

#[cfg(not(target_arch = "x86_64"))]
use core::ffi::c_double;
use core::ffi::c_longlong;
use core::ptr;

use raven_libc_abi::structs::Tm as AbiTm;

use super::{convert, fill_abi_tm, fill_abi_tm_zone, read_abi_tm, static_tm_zone, zone};

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
    match zone::with_cache(|cache| {
        let state = cache.state();
        convert::localtime_r(state, seconds)
            .map(|tm| (tm, zone::abbreviation(cache, tm.tm_isdst > 0)))
    }) {
        Ok((tm, zone_pointer)) => static_tm_zone(&tm, zone_pointer),
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
    match zone::with_cache(|cache| {
        raven_libc_time::tz::mktime::mktime(cache.state(), &mut value)
            .map(|seconds| (seconds, zone::abbreviation(cache, value.tm_isdst > 0)))
    }) {
        Ok((seconds, zone_pointer)) => {
            fill_abi_tm_zone(source, &value, zone_pointer);
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

/// `difftime`.
#[cfg(not(target_arch = "x86_64"))]
#[unsafe(no_mangle)]
pub extern "C" fn difftime(t1: c_longlong, t0: c_longlong) -> c_double {
    raven_libc_time::difftime::difftime(t1, t0)
}

// The bare x86_64 target has no SSE, so Rust returns an `f64` in `rax`; the C `time` case is
// built with the normal SSE ABI and expects the result in `xmm0`. The engine itself stays
// soft-float; this tiny thunk is the ABI boundary for `difftime` only.
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    ".global difftime",
    ".type difftime,@function",
    "difftime:",
    "subq $8, %rsp",
    "call {target}",
    "addq $8, %rsp",
    "movq %rax, %xmm0",
    "ret",
    ".size difftime, .-difftime",
    target = sym raven_libc_time::difftime::difftime,
    options(att_syntax),
);
