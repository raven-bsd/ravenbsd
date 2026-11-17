// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The aarch64 `long double` (binary128) scalar exports.
//!
//! On aarch64 the AAPCS64 passes and returns a `long double` in one 128-bit SIMD register
//! (`q0`, `q1`, ... for further arguments); Rust has no `f128` on stable, so every export is a
//! naked stub (the macros in `math/mod.rs`) that stores the incoming `q` registers on the stack,
//! calls a pointer-based `extern "C"` helper and reloads the result into `q0`. The helpers take and
//! return `raven_libc_m::LongDouble` (the binary128 value type) by pointer, which keeps the
//! aggregate out of the register ABI entirely.
//!
//! Only the operations `LongDouble128` implements are exported. The transcendentals and the
//! fused/arithmetic families are listed in the C47m report with their owners; no stub returns a
//! placeholder value.
//!
//! # Archive layout
//!
//! The archive contract allows at most four global text symbols per member, and Rust partitions
//! codegen units per module. The exports are therefore grouped into eight small submodules of at
//! most four names each, which is what the `mkmap` K4 check verifies.

use raven_libc_m::LongDouble;

/// Value/rounding exports: `fabsl`, `floorl`, `ceill`, `truncl`.
pub mod g1 {
    use raven_libc_m::LongDouble;

    extern "C" fn fabsl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::fabsl(*x);
        }
    }

    extern "C" fn floorl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::floorl(*x);
        }
    }

    extern "C" fn ceill_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::ceill(*x);
        }
    }

    extern "C" fn truncl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::truncl(*x);
        }
    }

    lda_unary!(fabsl, fabsl_inner);
    lda_unary!(floorl, floorl_inner);
    lda_unary!(ceill, ceill_inner);
    lda_unary!(truncl, truncl_inner);
}

/// Rounding and `logb` exports: `roundl`, `rintl`, `nearbyintl`, `logbl`.
pub mod g2 {
    use raven_libc_m::LongDouble;

    extern "C" fn roundl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::roundl(*x);
        }
    }

    extern "C" fn rintl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::rintl(*x);
        }
    }

    extern "C" fn nearbyintl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::nearbyintl(*x);
        }
    }

    extern "C" fn logbl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::logbl(*x);
        }
    }

    lda_unary!(roundl, roundl_inner);
    lda_unary!(rintl, rintl_inner);
    lda_unary!(nearbyintl, nearbyintl_inner);
    lda_unary!(logbl, logbl_inner);
}

/// Significand, sign and max/min exports: `significandl`, `copysignl`, `fdiml`, `fmaxl`.
pub mod g3 {
    use raven_libc_m::LongDouble;

    extern "C" fn significandl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::significandl(*x);
        }
    }

    extern "C" fn copysignl_inner(
        out: *mut LongDouble,
        x: *const LongDouble,
        y: *const LongDouble,
    ) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::copysignl(*x, *y);
        }
    }

    extern "C" fn fdiml_inner(out: *mut LongDouble, x: *const LongDouble, y: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::fdiml(*x, *y);
        }
    }

    extern "C" fn fmaxl_inner(out: *mut LongDouble, x: *const LongDouble, y: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::fmaxl(*x, *y);
        }
    }

    lda_unary!(significandl, significandl_inner);
    lda_binary!(copysignl, copysignl_inner);
    lda_binary!(fdiml, fdiml_inner);
    lda_binary!(fmaxl, fmaxl_inner);
}

/// Min, remainder and next-value exports: `fminl`, `fmodl`, `remainderl`, `nextafterl`.
pub mod g4 {
    use raven_libc_m::LongDouble;

    extern "C" fn fminl_inner(out: *mut LongDouble, x: *const LongDouble, y: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::fminl(*x, *y);
        }
    }

    extern "C" fn fmodl_inner(out: *mut LongDouble, x: *const LongDouble, y: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::fmodl(*x, *y);
        }
    }

    extern "C" fn remainderl_inner(
        out: *mut LongDouble,
        x: *const LongDouble,
        y: *const LongDouble,
    ) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::remainderl(*x, *y);
        }
    }

    extern "C" fn nextafterl_inner(
        out: *mut LongDouble,
        x: *const LongDouble,
        y: *const LongDouble,
    ) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::nextafterl(*x, *y);
        }
    }

    lda_binary!(fminl, fminl_inner);
    lda_binary!(fmodl, fmodl_inner);
    lda_binary!(remainderl, remainderl_inner);
    lda_binary!(nextafterl, nextafterl_inner);
}

/// Next-toward and scaling exports: `nexttowardl`, `scalbnl`, `ldexpl`, `scalblnl`.
pub mod g5 {
    use core::ffi::c_int;

    use raven_libc_m::LongDouble;

    extern "C" fn nexttowardl_inner(
        out: *mut LongDouble,
        x: *const LongDouble,
        y: *const LongDouble,
    ) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::nexttowardl(*x, *y);
        }
    }

    extern "C" fn scalbnl_inner(out: *mut LongDouble, x: *const LongDouble, n: c_int) {
        // SAFETY: the naked stub supplies a writable slot and the caller's int.
        unsafe {
            *out = raven_libc_m::scalbnl(*x, n);
        }
    }

    extern "C" fn ldexpl_inner(out: *mut LongDouble, x: *const LongDouble, n: c_int) {
        // SAFETY: the naked stub supplies a writable slot and the caller's int.
        unsafe {
            *out = raven_libc_m::ldexpl(*x, n);
        }
    }

    extern "C" fn scalblnl_inner(out: *mut LongDouble, x: *const LongDouble, n: i64) {
        // SAFETY: the naked stub supplies a writable slot and the caller's long.
        unsafe {
            *out = raven_libc_m::scalblnl(*x, n);
        }
    }

    lda_binary!(nexttowardl, nexttowardl_inner);
    lda_scalbn_i32!(scalbnl, scalbnl_inner);
    lda_scalbn_i32!(ldexpl, ldexpl_inner);
    lda_scalbn_i64!(scalblnl, scalblnl_inner);
}

/// Decomposition exports: `frexpl`, `modfl`, `remquol`, `ilogbl`.
pub mod g6 {
    use core::ffi::c_int;

    use raven_libc_m::LongDouble;

    extern "C" fn frexpl_inner(out: *mut LongDouble, x: *const LongDouble, exponent: *mut c_int) {
        // SAFETY: the naked stub passes a readable 16-byte argument slot.
        let (mantissa, value) = raven_libc_m::frexpl(unsafe { *x });
        // SAFETY: the naked stub passes the caller's writable int slot.
        unsafe {
            *out = mantissa;
            if !exponent.is_null() {
                *exponent = value;
            }
        }
    }

    extern "C" fn modfl_inner(
        out: *mut LongDouble,
        x: *const LongDouble,
        integral: *mut LongDouble,
    ) {
        // SAFETY: the naked stub passes a readable 16-byte argument slot.
        let (fraction, whole) = raven_libc_m::modfl(unsafe { *x });
        // SAFETY: the naked stub passes the caller's writable long-double slot.
        unsafe {
            *out = fraction;
            if !integral.is_null() {
                *integral = whole;
            }
        }
    }

    extern "C" fn remquol_inner(
        out: *mut LongDouble,
        x: *const LongDouble,
        y: *const LongDouble,
        quotient: *mut c_int,
    ) {
        // SAFETY: the naked stub passes two readable 16-byte argument slots.
        let (value, quotient_value) = raven_libc_m::remquol(unsafe { *x }, unsafe { *y });
        // SAFETY: the naked stub passes the caller's writable int slot.
        unsafe {
            *out = value;
            if !quotient.is_null() {
                *quotient = quotient_value;
            }
        }
    }

    extern "C" fn ilogbl_inner(x: *const LongDouble) -> c_int {
        // SAFETY: the naked stub passes a readable 16-byte argument slot.
        raven_libc_m::ilogbl(unsafe { *x })
    }

    lda_value_ptr!(frexpl, frexpl_inner);
    lda_value_ptr!(modfl, modfl_inner);
    lda_remquo!(remquol, remquol_inner);
    lda_to_i32!(ilogbl, ilogbl_inner);
}

/// `llogbl` and the integer-rounding exports: `llogbl`, `lrintl`, `llrintl`, `lroundl`.
pub mod g7 {
    use raven_libc_m::LongDouble;

    extern "C" fn llogbl_inner(x: *const LongDouble) -> i64 {
        // SAFETY: the naked stub passes a readable 16-byte argument slot.
        raven_libc_m::llogbl(unsafe { *x })
    }

    extern "C" fn lrintl_inner(x: *const LongDouble) -> i64 {
        // SAFETY: the naked stub passes a readable 16-byte argument slot.
        raven_libc_m::lrintl(unsafe { *x })
    }

    extern "C" fn llrintl_inner(x: *const LongDouble) -> i64 {
        // SAFETY: the naked stub passes a readable 16-byte argument slot.
        raven_libc_m::llrintl(unsafe { *x })
    }

    extern "C" fn lroundl_inner(x: *const LongDouble) -> i64 {
        // SAFETY: the naked stub passes a readable 16-byte argument slot.
        raven_libc_m::lroundl(unsafe { *x })
    }

    lda_to_i64!(llogbl, llogbl_inner);
    lda_to_i64!(lrintl, lrintl_inner);
    lda_to_i64!(llrintl, llrintl_inner);
    lda_to_i64!(lroundl, lroundl_inner);
}

/// `llroundl` and `nanl`.
pub mod g8 {
    use core::ffi::c_char;

    use raven_libc_m::LongDouble;

    extern "C" fn llroundl_inner(x: *const LongDouble) -> i64 {
        // SAFETY: the naked stub passes a readable 16-byte argument slot.
        raven_libc_m::llroundl(unsafe { *x })
    }

    extern "C" fn nanl_inner(out: *mut LongDouble, tag: *const c_char) {
        // The tag is advisory in C; the binary128 payload is the canonical quiet NaN.
        let _ = tag;
        // SAFETY: the naked stub supplies a writable slot.
        unsafe {
            *out = raven_libc_m::nanl();
        }
    }

    extern "C" fn sqrtl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::sqrtl(*x);
        }
    }

    extern "C" fn cbrtl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::cbrtl(*x);
        }
    }

    lda_to_i64!(llroundl, llroundl_inner);
    lda_nan!(nanl, nanl_inner);
    lda_unary!(sqrtl, sqrtl_inner);
    lda_unary!(cbrtl, cbrtl_inner);
}

/// `hypotl`.
pub mod g9 {
    use raven_libc_m::LongDouble;

    extern "C" fn hypotl_inner(out: *mut LongDouble, x: *const LongDouble, y: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::hypotl(*x, *y);
        }
    }

    lda_binary!(hypotl, hypotl_inner);
}

/// exp/log exports: `expl`, `logl`, `exp2l`, `log2l`.
pub mod g10 {
    use raven_libc_m::LongDouble;

    extern "C" fn expl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::expl(*x);
        }
    }

    extern "C" fn logl_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::logl(*x);
        }
    }

    extern "C" fn exp2l_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::exp2l(*x);
        }
    }

    extern "C" fn log2l_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::log2l(*x);
        }
    }

    lda_unary!(expl, expl_inner);
    lda_unary!(logl, logl_inner);
    lda_unary!(exp2l, exp2l_inner);
    lda_unary!(log2l, log2l_inner);
}

/// `log10l`.
pub mod g11 {
    use raven_libc_m::LongDouble;

    extern "C" fn log10l_inner(out: *mut LongDouble, x: *const LongDouble) {
        // SAFETY: the naked stub supplies writable and readable 16-byte slots.
        unsafe {
            *out = raven_libc_m::log10l(*x);
        }
    }

    lda_unary!(log10l, log10l_inner);
}
