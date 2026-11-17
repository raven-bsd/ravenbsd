// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The aarch64 `long double` (binary128) scalar exports.
//!
//! On aarch64 the AAPCS64 passes and returns a `long double` in one 128-bit SIMD register
//! (`q0`, `q1`, ... for further arguments); Rust has no `f128` on stable, so every export is a
//! naked stub that stores the incoming `q` registers on the stack, calls a pointer-based
//! `extern "C"` helper and reloads the result into `q0`. The helpers take and return
//! `raven_libc_m::LongDouble` (the binary128 value type) by pointer, which keeps the aggregate out
//! of the register ABI entirely.
//!
//! Only the operations `LongDouble128` implements are exported. The transcendentals and the
//! fused/arithmetic families are listed in the C47m report with their owners; no stub returns a
//! placeholder value.

use core::ffi::{c_char, c_int};

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

/// A naked stub whose argument and result are both `long double` in `q0`.
macro_rules! ld_unary {
    ($name:ident, $inner:path) => {
        /// `long double` unary entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                ".arch armv8-a+fp+simd",
                "sub sp, sp, #48",
                "str x30, [sp, #32]",
                "str q0, [sp, #16]",
                "mov x0, sp",
                "add x1, sp, #16",
                "bl {inner}",
                "ldr q0, [sp, #0]",
                "ldr x30, [sp, #32]",
                "add sp, sp, #48",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

/// A naked stub with two `long double` arguments (`q0`, `q1`) and the result in `q0`.
macro_rules! ld_binary {
    ($name:ident, $inner:path) => {
        /// `long double` binary entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                ".arch armv8-a+fp+simd",
                "sub sp, sp, #64",
                "str x30, [sp, #48]",
                "str q0, [sp, #16]",
                "str q1, [sp, #32]",
                "mov x0, sp",
                "add x1, sp, #16",
                "add x2, sp, #32",
                "bl {inner}",
                "ldr q0, [sp, #0]",
                "ldr x30, [sp, #48]",
                "add sp, sp, #64",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

/// `long double` value plus a C `int` (in `w0`).
macro_rules! ld_scalbn_i32 {
    ($name:ident, $inner:path) => {
        /// `long double` scaling entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                ".arch armv8-a+fp+simd",
                "sub sp, sp, #48",
                "str x30, [sp, #32]",
                "str q0, [sp, #16]",
                "mov w2, w0",
                "mov x0, sp",
                "add x1, sp, #16",
                "bl {inner}",
                "ldr q0, [sp, #0]",
                "ldr x30, [sp, #32]",
                "add sp, sp, #48",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

/// `long double` value plus a C `long` (in `x0`).
macro_rules! ld_scalbn_i64 {
    ($name:ident, $inner:path) => {
        /// `long double` scaling entry point with a C `long` exponent.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                ".arch armv8-a+fp+simd",
                "sub sp, sp, #48",
                "str x30, [sp, #32]",
                "str q0, [sp, #16]",
                "mov x2, x0",
                "mov x0, sp",
                "add x1, sp, #16",
                "bl {inner}",
                "ldr q0, [sp, #0]",
                "ldr x30, [sp, #32]",
                "add sp, sp, #48",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

/// `long double` value plus a writable pointer (in `x0`) for the `frexp`/`modf` families.
macro_rules! ld_value_ptr {
    ($name:ident, $inner:path) => {
        /// `long double` value/pointer entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                ".arch armv8-a+fp+simd",
                "sub sp, sp, #48",
                "str x30, [sp, #32]",
                "str q0, [sp, #16]",
                "mov x2, x0",
                "mov x0, sp",
                "add x1, sp, #16",
                "bl {inner}",
                "ldr q0, [sp, #0]",
                "ldr x30, [sp, #32]",
                "add sp, sp, #48",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

/// `remquol`: two `long double`s plus the quotient pointer.
macro_rules! ld_remquo {
    ($name:ident, $inner:path) => {
        /// `remquol` entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                ".arch armv8-a+fp+simd",
                "sub sp, sp, #64",
                "str x30, [sp, #48]",
                "str q0, [sp, #16]",
                "str q1, [sp, #32]",
                "mov x3, x0",
                "mov x0, sp",
                "add x1, sp, #16",
                "add x2, sp, #32",
                "bl {inner}",
                "ldr q0, [sp, #0]",
                "ldr x30, [sp, #48]",
                "add sp, sp, #64",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

/// `long double` argument, `int` result (`w0`).
macro_rules! ld_to_i32 {
    ($name:ident, $inner:path) => {
        /// `long double` to `int` entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                ".arch armv8-a+fp+simd",
                "sub sp, sp, #32",
                "str x30, [sp, #16]",
                "str q0, [sp, #0]",
                "mov x0, sp",
                "bl {inner}",
                "ldr x30, [sp, #16]",
                "add sp, sp, #32",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

/// `long double` argument, C `long` result (`x0`).
macro_rules! ld_to_i64 {
    ($name:ident, $inner:path) => {
        /// `long double` to `long` entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                ".arch armv8-a+fp+simd",
                "sub sp, sp, #32",
                "str x30, [sp, #16]",
                "str q0, [sp, #0]",
                "mov x0, sp",
                "bl {inner}",
                "ldr x30, [sp, #16]",
                "add sp, sp, #32",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

/// `nanl`: the tag is advisory (`const char *` in `x0`), the result is a quiet NaN in `q0`.
macro_rules! ld_nan {
    ($name:ident, $inner:path) => {
        /// `nanl` entry point.
        ///
        /// # Safety
        /// `tag` is NULL or a NUL-terminated string.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                ".arch armv8-a+fp+simd",
                "sub sp, sp, #32",
                "str x30, [sp, #16]",
                "mov x1, x0",
                "mov x0, sp",
                "bl {inner}",
                "ldr q0, [sp, #0]",
                "ldr x30, [sp, #16]",
                "add sp, sp, #32",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

unary_inner!(fabsl_inner, raven_libc_m::fabsl);
unary_inner!(floorl_inner, raven_libc_m::floorl);
unary_inner!(ceill_inner, raven_libc_m::ceill);
unary_inner!(truncl_inner, raven_libc_m::truncl);
unary_inner!(roundl_inner, raven_libc_m::roundl);
unary_inner!(rintl_inner, raven_libc_m::rintl);
unary_inner!(nearbyintl_inner, raven_libc_m::nearbyintl);
unary_inner!(logbl_inner, raven_libc_m::logbl);
unary_inner!(significandl_inner, raven_libc_m::significandl);

binary_inner!(copysignl_inner, raven_libc_m::copysignl);
binary_inner!(fdiml_inner, raven_libc_m::fdiml);
binary_inner!(fmaxl_inner, raven_libc_m::fmaxl);
binary_inner!(fminl_inner, raven_libc_m::fminl);
binary_inner!(fmodl_inner, raven_libc_m::fmodl);
binary_inner!(remainderl_inner, raven_libc_m::remainderl);
binary_inner!(nextafterl_inner, raven_libc_m::nextafterl);
binary_inner!(nexttowardl_inner, raven_libc_m::nexttowardl);

ld_unary!(fabsl, fabsl_inner);
ld_unary!(floorl, floorl_inner);
ld_unary!(ceill, ceill_inner);
ld_unary!(truncl, truncl_inner);
ld_unary!(roundl, roundl_inner);
ld_unary!(rintl, rintl_inner);
ld_unary!(nearbyintl, nearbyintl_inner);
ld_unary!(logbl, logbl_inner);
ld_unary!(significandl, significandl_inner);

ld_binary!(copysignl, copysignl_inner);
ld_binary!(fdiml, fdiml_inner);
ld_binary!(fmaxl, fmaxl_inner);
ld_binary!(fminl, fminl_inner);
ld_binary!(fmodl, fmodl_inner);
ld_binary!(remainderl, remainderl_inner);
ld_binary!(nextafterl, nextafterl_inner);
ld_binary!(nexttowardl, nexttowardl_inner);

pub(super) extern "C" fn scalbnl_inner(out: *mut LongDouble, x: *const LongDouble, n: c_int) {
    // SAFETY: the naked stub supplies a writable slot and the caller's int.
    unsafe {
        *out = raven_libc_m::scalbnl(*x, n);
    }
}

pub(super) extern "C" fn ldexpl_inner(out: *mut LongDouble, x: *const LongDouble, n: c_int) {
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
    exponent: *mut c_int,
) {
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
        if !integral.is_null() {
            *integral = whole;
        }
    }
}

pub(super) extern "C" fn remquol_inner(
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

pub(super) extern "C" fn ilogbl_inner(x: *const LongDouble) -> c_int {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    raven_libc_m::ilogbl(unsafe { *x })
}

pub(super) extern "C" fn llogbl_inner(x: *const LongDouble) -> i64 {
    // SAFETY: the naked stub passes a readable 16-byte argument slot.
    raven_libc_m::llogbl(unsafe { *x })
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

pub(super) extern "C" fn nanl_inner(out: *mut LongDouble, tag: *const c_char) {
    // The tag is advisory in C; the binary128 payload is the canonical quiet NaN.
    let _ = tag;
    // SAFETY: the naked stub supplies a writable slot.
    unsafe {
        *out = raven_libc_m::nanl();
    }
}

ld_scalbn_i32!(scalbnl, scalbnl_inner);
ld_scalbn_i32!(ldexpl, ldexpl_inner);
ld_scalbn_i64!(scalblnl, scalblnl_inner);
ld_value_ptr!(frexpl, frexpl_inner);
ld_value_ptr!(modfl, modfl_inner);
ld_remquo!(remquol, remquol_inner);
ld_to_i32!(ilogbl, ilogbl_inner);
ld_to_i64!(llogbl, llogbl_inner);
ld_to_i64!(lrintl, lrintl_inner);
ld_to_i64!(llrintl, llrintl_inner);
ld_to_i64!(lroundl, lroundl_inner);
ld_to_i64!(llroundl, llroundl_inner);
ld_nan!(nanl, nanl_inner);
