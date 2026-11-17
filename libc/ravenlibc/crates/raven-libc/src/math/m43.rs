// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The C `long double` scalar exports.
//!
//! Every symbol here is a naked C-ABI stub over a pointer-based helper; the macros in
//! `math/mod.rs` explain the x87 memory-slot/st(0) contract.

use core::ffi::c_char;

use raven_libc_m::LongDouble;

macro_rules! unary_inner {
    ($inner:ident, $call:path) => {
        pub(super) extern "C" fn $inner(out: *mut LongDouble, x: *const LongDouble) {
            // SAFETY: the naked stub supplies writable and readable 16-byte slots.
            unsafe {
                *out = $call(*x);
            }
        }
    };
}

macro_rules! binary_inner {
    ($inner:ident, $call:path) => {
        pub(super) extern "C" fn $inner(
            out: *mut LongDouble,
            x: *const LongDouble,
            y: *const LongDouble,
        ) {
            // SAFETY: the naked stub supplies writable and readable 16-byte slots.
            unsafe {
                *out = $call(*x, *y);
            }
        }
    };
}

unary_inner!(acosl_inner, raven_libc_m::acosl);
unary_inner!(acoshl_inner, raven_libc_m::acoshl);
unary_inner!(asinl_inner, raven_libc_m::asinl);
unary_inner!(asinhl_inner, raven_libc_m::asinhl);
unary_inner!(atanl_inner, raven_libc_m::atanl);
unary_inner!(atanhl_inner, raven_libc_m::atanhl);
unary_inner!(cbrtl_inner, raven_libc_m::cbrtl);
unary_inner!(ceill_inner, raven_libc_m::ceill);
unary_inner!(cosl_inner, raven_libc_m::cosl);
unary_inner!(coshl_inner, raven_libc_m::coshl);
unary_inner!(erfl_inner, raven_libc_m::erfl);
unary_inner!(erfcl_inner, raven_libc_m::erfcl);
unary_inner!(exp2l_inner, raven_libc_m::exp2l);
unary_inner!(exp10l_inner, raven_libc_m::exp10l);
unary_inner!(expl_inner, raven_libc_m::expl);
unary_inner!(expm1l_inner, raven_libc_m::expm1l);
unary_inner!(fabsl_inner, raven_libc_m::fabsl);
unary_inner!(floorl_inner, raven_libc_m::floorl);
unary_inner!(gammal_inner, raven_libc_m::lgammal);
unary_inner!(lgammal_inner, raven_libc_m::lgammal);
unary_inner!(log10l_inner, raven_libc_m::log10l);
unary_inner!(log1pl_inner, raven_libc_m::log1pl);
unary_inner!(log2l_inner, raven_libc_m::log2l);
unary_inner!(logbl_inner, raven_libc_m::logbl);
unary_inner!(logl_inner, raven_libc_m::logl);
unary_inner!(nearbyintl_inner, raven_libc_m::nearbyintl);
unary_inner!(pow10l_inner, raven_libc_m::pow10l);
unary_inner!(rintl_inner, raven_libc_m::rintl);
unary_inner!(roundl_inner, raven_libc_m::roundl);
unary_inner!(significandl_inner, raven_libc_m::significandl);
unary_inner!(sinl_inner, raven_libc_m::sinl);
unary_inner!(sinhl_inner, raven_libc_m::sinhl);
unary_inner!(sqrtl_inner, raven_libc_m::sqrtl);
unary_inner!(tanl_inner, raven_libc_m::tanl);
unary_inner!(tanhl_inner, raven_libc_m::tanhl);
unary_inner!(tgammal_inner, raven_libc_m::tgammal);
unary_inner!(truncl_inner, raven_libc_m::truncl);

binary_inner!(atan2l_inner, raven_libc_m::atan2l);
binary_inner!(copysignl_inner, raven_libc_m::copysignl);
binary_inner!(fdiml_inner, raven_libc_m::fdiml);
binary_inner!(fmaxl_inner, raven_libc_m::fmaxl);
binary_inner!(fminl_inner, raven_libc_m::fminl);
binary_inner!(fmodl_inner, raven_libc_m::fmodl);
binary_inner!(hypotl_inner, raven_libc_m::hypotl);
binary_inner!(nextafterl_inner, raven_libc_m::nextafterl);
binary_inner!(nexttowardl_inner, raven_libc_m::nexttowardl);
binary_inner!(powl_inner, raven_libc_m::powl);
binary_inner!(remainderl_inner, raven_libc_m::remainderl);

pub(super) extern "C" fn fmal_inner(
    out: *mut LongDouble,
    x: *const LongDouble,
    y: *const LongDouble,
    z: *const LongDouble,
) {
    // SAFETY: the naked stub supplies writable and readable 16-byte slots.
    unsafe {
        *out = raven_libc_m::fmal(*x, *y, *z);
    }
}

pub(super) extern "C" fn scalbnl_inner(out: *mut LongDouble, x: *const LongDouble, n: i32) {
    // SAFETY: the naked stub supplies a writable slot and the caller's int.
    unsafe {
        *out = raven_libc_m::scalbnl(*x, n);
    }
}

pub(super) extern "C" fn ldexpl_inner(out: *mut LongDouble, x: *const LongDouble, n: i32) {
    // SAFETY: the naked stub supplies a writable slot and the caller's int.
    unsafe {
        *out = raven_libc_m::ldexpl(*x, n);
    }
}

pub(super) extern "C" fn scalblnl_inner(out: *mut LongDouble, x: *const LongDouble, n: i64) {
    // SAFETY: the naked stub supplies a writable slot and the caller's long.
    unsafe {
        *out = raven_libc_m::scalblnl(*x, n);
    }
}

pub(super) extern "C" fn frexpl_inner(
    out: *mut LongDouble,
    x: *const LongDouble,
    exponent: *mut core::ffi::c_int,
) {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    let (mantissa, value) = raven_libc_m::frexpl(unsafe { *x });
    // SAFETY: the naked stub passes the callers's writable int slot.
    unsafe {
        *out = mantissa;
        if !exponent.is_null() {
            *exponent = value;
        }
    }
}

pub(super) extern "C" fn modfl_inner(
    out: *mut LongDouble,
    x: *const LongDouble,
    integral: *mut LongDouble,
) {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    let (fraction, whole) = raven_libc_m::modfl(unsafe { *x });
    // SAFETY: the naked stub passes the caller's writable long-double slot.
    unsafe {
        *out = fraction;
        *integral = whole;
    }
}

pub(super) extern "C" fn lgammal_r_inner(
    out: *mut LongDouble,
    x: *const LongDouble,
    sign: *mut core::ffi::c_int,
) {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    let (value, sign_value) = raven_libc_m::lgamma_rl(unsafe { *x });
    // SAFETY: the naked stub passes the caller's writable int slot.
    unsafe {
        *out = value;
        *sign = sign_value;
    }
}

pub(super) extern "C" fn remquol_inner(
    out: *mut LongDouble,
    x: *const LongDouble,
    y: *const LongDouble,
    quotient: *mut core::ffi::c_int,
) {
    // SAFETY: the naked stub passes two readable 16-byte argument slots.
    let (value, quotient_value) = raven_libc_m::remquol(unsafe { *x }, unsafe { *y });
    // SAFETY: the naked stub passes the caller's writable int slot.
    unsafe {
        *out = value;
        *quotient = quotient_value;
    }
}

pub(super) extern "C" fn sincosl_inner(
    sine: *mut LongDouble,
    cosine: *mut LongDouble,
    x: *const LongDouble,
) {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    let (sin_value, cos_value) = raven_libc_m::sincosl(unsafe { *x });
    // SAFETY: the naked stub passes the caller's two writable long-double slots.
    unsafe {
        *sine = sin_value;
        *cosine = cos_value;
    }
}

pub(super) extern "C" fn ilogbl_inner(x: *const LongDouble) -> i32 {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    raven_libc_m::ilogbl(unsafe { *x })
}

pub(super) extern "C" fn lrintl_inner(x: *const LongDouble) -> i64 {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    raven_libc_m::lrintl(unsafe { *x })
}

pub(super) extern "C" fn llrintl_inner(x: *const LongDouble) -> i64 {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    raven_libc_m::llrintl(unsafe { *x })
}

pub(super) extern "C" fn lroundl_inner(x: *const LongDouble) -> i64 {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    raven_libc_m::lroundl(unsafe { *x })
}

pub(super) extern "C" fn llroundl_inner(x: *const LongDouble) -> i64 {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    raven_libc_m::llroundl(unsafe { *x })
}

pub(super) extern "C" fn llogbl_inner(x: *const LongDouble) -> i64 {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    raven_libc_m::llogbl(unsafe { *x })
}

pub(super) extern "C" fn nanl_inner(out: *mut LongDouble, tag: *const c_char) {
    let bytes = if tag.is_null() {
        &[][..]
    } else {
        // SAFETY: the caller's string contract.
        unsafe { crate::ffi::cstr::slice(tag) }
    };
    // SAFETY: the naked stub supplies a writable slot.
    unsafe {
        *out = raven_libc_m::nanl_with_tag(bytes);
    }
}

ld_unary!(acosl, acosl_inner);
ld_unary!(acoshl, acoshl_inner);
ld_unary!(asinl, asinl_inner);
ld_unary!(asinhl, asinhl_inner);
