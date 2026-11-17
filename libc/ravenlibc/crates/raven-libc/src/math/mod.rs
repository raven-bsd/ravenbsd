// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `<math.h>` scalar exports: every body is one `raven-libc-m` call.
//!
//! The `f64`/`f32` families that C47a/C47b provide are bound; the `*l` long-double rows stay in
//! `expected/DEFERRED.txt` until C47c (no aliasing to `f64`). C23/legacy names the crate provides
//! are exported under their C spelling; complex `c*` entry points are not bound in this pass.
//!
//! The functions are split over small modules because the archive contract allows at most four C
//! text symbols per object member.

macro_rules! unary {
    ($name:ident, $call:path) => {
        /// Scalar math entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: core::ffi::c_double) -> core::ffi::c_double {
            $call(x)
        }
    };
}

macro_rules! unaryf {
    ($name:ident, $call:path) => {
        /// Scalar math entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: core::ffi::c_float) -> core::ffi::c_float {
            $call(x)
        }
    };
}

macro_rules! binary {
    ($name:ident, $call:path) => {
        /// Binary scalar math entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            x: core::ffi::c_double,
            y: core::ffi::c_double,
        ) -> core::ffi::c_double {
            $call(x, y)
        }
    };
}

macro_rules! binaryf {
    ($name:ident, $call:path) => {
        /// Binary scalar math entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            x: core::ffi::c_float,
            y: core::ffi::c_float,
        ) -> core::ffi::c_float {
            $call(x, y)
        }
    };
}

macro_rules! scalbn {
    ($name:ident, $call:path) => {
        /// `scalbn`-family entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            x: core::ffi::c_double,
            n: core::ffi::c_int,
        ) -> core::ffi::c_double {
            $call(x, n)
        }
    };
}

macro_rules! scalbnf {
    ($name:ident, $call:path) => {
        /// `scalbnf`-family entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: core::ffi::c_float, n: core::ffi::c_int) -> core::ffi::c_float {
            $call(x, n)
        }
    };
}

macro_rules! unary_i64 {
    ($name:ident, $call:path) => {
        /// Integer-returning unary entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: core::ffi::c_double) -> i64 {
            $call(x)
        }
    };
}

macro_rules! unaryf_i64 {
    ($name:ident, $call:path) => {
        /// Integer-returning f32 entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: core::ffi::c_float) -> i64 {
            $call(x)
        }
    };
}

macro_rules! unary_i32 {
    ($name:ident, $call:path) => {
        /// Predicate entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: core::ffi::c_double) -> core::ffi::c_int {
            $call(x)
        }
    };
}

macro_rules! binary_i32 {
    ($name:ident, $call:path) => {
        /// Binary predicate entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            x: core::ffi::c_double,
            y: core::ffi::c_double,
        ) -> core::ffi::c_int {
            $call(x, y)
        }
    };
}

macro_rules! scalbln_i64 {
    ($name:ident, $call:path, $ty:ty) => {
        /// `scalbln` entry point with a C `long` exponent.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: $ty, n: i64) -> $ty {
            $call(x, n)
        }
    };
}

macro_rules! remquo {
    ($name:ident, $call:path, $ty:ty) => {
        /// `remquo` entry point.
        ///
        /// # Safety
        /// `quo` is a writable `int`.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(x: $ty, y: $ty, quo: *mut core::ffi::c_int) -> $ty {
            // SAFETY: the C contract requires a writable quotient slot.
            let quotient = unsafe { &mut *quo };
            $call(x, y, quotient)
        }
    };
}

macro_rules! pown {
    ($name:ident, $call:path, $ty:ty) => {
        /// `pown` entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: $ty, n: core::ffi::c_int) -> $ty {
            $call(x, n)
        }
    };
}

macro_rules! compoundn {
    ($name:ident, $call:path, $ty:ty) => {
        /// `compoundn` entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(x: $ty, n: i64) -> $ty {
            $call(x, n)
        }
    };
}

macro_rules! fromfp {
    ($name:ident, $call:path, $ty:ty) => {
        /// `fromfp`/`ufromfp` entry point.
        #[unsafe(no_mangle)]
        pub extern "C" fn $name(
            x: core::ffi::c_double,
            mode: core::ffi::c_int,
            width: core::ffi::c_uint,
        ) -> $ty {
            $call(x, mode, width)
        }
    };
}

macro_rules! nan {
    ($name:ident, $call:path, $ty:ty) => {
        /// `nan` entry point.
        ///
        /// # Safety
        /// `tag` is NULL or a NUL-terminated string.
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name(tag: *const core::ffi::c_char) -> $ty {
            let bytes = if tag.is_null() {
                &[][..]
            } else {
                // SAFETY: the caller's string contract.
                unsafe { crate::ffi::cstr::slice(tag) }
            };
            $call(bytes)
        }
    };
}

// The `long double` C ABI passes the 80-bit value in a 16-byte memory slot and returns it in
// `st(0)` (SysV x86-64, class X87/X87UP). Rust has no `f80`, so each export is a naked stub that
// hands the caller's 16-byte image to a pointer-based `extern "C"` helper and reloads the result
// with `fldt`. The helpers take and return `raven_libc_m::LongDouble` by pointer, which keeps the
// aggregate out of the register ABI entirely.
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
                "sub rsp, 24",
                "lea rdi, [rsp]",
                "lea rsi, [rsp + 32]",
                "call {inner}",
                "fld tbyte ptr [rsp]",
                "add rsp, 24",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

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
                "sub rsp, 24",
                "lea rdi, [rsp]",
                "lea rsi, [rsp + 32]",
                "lea rdx, [rsp + 48]",
                "call {inner}",
                "fld tbyte ptr [rsp]",
                "add rsp, 24",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

macro_rules! ld_ternary {
    ($name:ident, $inner:path) => {
        /// `long double` ternary entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                "sub rsp, 24",
                "lea rdi, [rsp]",
                "lea rsi, [rsp + 32]",
                "lea rdx, [rsp + 48]",
                "lea rcx, [rsp + 64]",
                "call {inner}",
                "fld tbyte ptr [rsp]",
                "add rsp, 24",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

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
                "sub rsp, 24",
                "mov edx, edi",
                "lea rdi, [rsp]",
                "lea rsi, [rsp + 32]",
                "call {inner}",
                "fld tbyte ptr [rsp]",
                "add rsp, 24",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

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
                "sub rsp, 24",
                "mov rdx, rdi",
                "lea rdi, [rsp]",
                "lea rsi, [rsp + 32]",
                "call {inner}",
                "fld tbyte ptr [rsp]",
                "add rsp, 24",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

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
                "sub rsp, 24",
                "mov rdx, rdi",
                "lea rdi, [rsp]",
                "lea rsi, [rsp + 32]",
                "call {inner}",
                "fld tbyte ptr [rsp]",
                "add rsp, 24",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

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
                "sub rsp, 24",
                "mov rcx, rdi",
                "lea rdi, [rsp]",
                "lea rsi, [rsp + 32]",
                "lea rdx, [rsp + 48]",
                "call {inner}",
                "fld tbyte ptr [rsp]",
                "add rsp, 24",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

macro_rules! ld_sincos {
    ($name:ident, $inner:path) => {
        /// `sincosl` entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                "sub rsp, 8",
                "lea rdx, [rsp + 16]",
                "call {inner}",
                "add rsp, 8",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

macro_rules! ld_to_i64 {
    ($name:ident, $inner:path) => {
        /// `long double` to integer entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                "sub rsp, 8",
                "lea rdi, [rsp + 16]",
                "call {inner}",
                "add rsp, 8",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

macro_rules! ld_to_i32 {
    ($name:ident, $inner:path) => {
        /// `long double` predicate entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                "sub rsp, 8",
                "lea rdi, [rsp + 16]",
                "call {inner}",
                "add rsp, 8",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

macro_rules! ld_nan {
    ($name:ident, $inner:path) => {
        /// `nanl` entry point.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                "sub rsp, 24",
                "mov rsi, rdi",
                "lea rdi, [rsp]",
                "call {inner}",
                "fld tbyte ptr [rsp]",
                "add rsp, 24",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

macro_rules! nexttoward {
    ($name:ident, $inner:path) => {
        /// `nexttoward` entry point.
        ///
        /// The `double`/`float` argument arrives in the first integer register (the soft-float
        /// C ABI) and the `long double` argument is the x87 memory slot at `[rsp + 8]`; the
        /// helper's second aggregate goes to the next two integer registers.
        ///
        /// # Safety
        /// The compiler-generated C ABI contract applies.
        #[unsafe(naked)]
        #[unsafe(no_mangle)]
        pub unsafe extern "C" fn $name() -> ! {
            core::arch::naked_asm!(
                "sub rsp, 24",
                "mov rsi, [rsp + 32]",
                "mov rdx, [rsp + 40]",
                "call {inner}",
                "add rsp, 24",
                "ret",
                inner = sym $inner,
            )
        }
    };
}

// The aarch64 binary128 `long double` C ABI (C47n). AAPCS64 passes and returns a `long double` in
// a single 128-bit SIMD register (`q0`, `q1`, ... for further arguments), and Rust has no `f128` on
// stable, so every export below is a naked stub: it parks the incoming `q` registers on the stack,
// calls a pointer-based `extern "C"` helper and reloads the result into `q0`. The helpers take and
// return `raven_libc_m::LongDouble` by pointer, which keeps the aggregate out of the register ABI.
#[cfg(target_arch = "aarch64")]
macro_rules! lda_unary {
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

#[cfg(target_arch = "aarch64")]
macro_rules! lda_binary {
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

#[cfg(target_arch = "aarch64")]
macro_rules! lda_scalbn_i32 {
    ($name:ident, $inner:path) => {
        /// `long double` scaling entry point with a C `int` exponent.
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

#[cfg(target_arch = "aarch64")]
macro_rules! lda_scalbn_i64 {
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

#[cfg(target_arch = "aarch64")]
macro_rules! lda_value_ptr {
    ($name:ident, $inner:path) => {
        /// `long double` value plus a writable pointer (in `x0`).
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

#[cfg(target_arch = "aarch64")]
macro_rules! lda_remquo {
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

#[cfg(target_arch = "aarch64")]
macro_rules! lda_to_i32 {
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

#[cfg(target_arch = "aarch64")]
macro_rules! lda_to_i64 {
    ($name:ident, $inner:path) => {
        /// `long double` to C `long` entry point.
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

#[cfg(target_arch = "aarch64")]
macro_rules! lda_nan {
    ($name:ident, $inner:path) => {
        /// `nanl` entry point; the tag is advisory.
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

mod fma;
mod frexp;
mod getpayload;
mod isinf;
mod lgamma_r;
mod m01;
mod m02;
mod m03;
mod m04;
mod m05;
mod m06;
mod m07;
mod m08;
mod m09;
mod m10;
mod m11;
mod m12;
mod m13;
mod m14;
mod m15;
mod m16;
mod m17;
mod m18;
mod m19;
mod m20;
mod m21;
mod m22;
mod m23;
mod m24;
mod m25;
mod m26;
mod m27;
mod m28;
mod m29;
mod m30;
mod m31;
mod m32;
mod m33;
mod m34;
mod m35;
mod m36;
mod m37;
mod m38;
mod m39;
mod m40;
mod m41;
mod m42;
#[cfg(target_arch = "x86_64")]
mod m43;
#[cfg(target_arch = "x86_64")]
mod m44;
#[cfg(target_arch = "x86_64")]
mod m45;
#[cfg(target_arch = "x86_64")]
mod m46;
#[cfg(target_arch = "x86_64")]
mod m47;
#[cfg(target_arch = "x86_64")]
mod m48;
#[cfg(target_arch = "x86_64")]
mod m49;
#[cfg(target_arch = "x86_64")]
mod m50;
#[cfg(target_arch = "x86_64")]
mod m51;
#[cfg(target_arch = "x86_64")]
mod m52;
#[cfg(target_arch = "x86_64")]
mod m53;
#[cfg(target_arch = "x86_64")]
mod m54;
#[cfg(target_arch = "x86_64")]
mod m55;
#[cfg(target_arch = "x86_64")]
mod m56;
#[cfg(target_arch = "x86_64")]
mod m57;
#[cfg(target_arch = "x86_64")]
mod m58;
#[cfg(target_arch = "x86_64")]
mod m59;
mod m60;
mod m61;
mod m62;
mod m63;
mod m64;
mod m65;
mod m66;
mod m67;
mod m68;
mod m69;
#[cfg(target_arch = "aarch64")]
mod m70;
mod sincos;
mod strfromd;
mod totalordermag;
mod yn;
