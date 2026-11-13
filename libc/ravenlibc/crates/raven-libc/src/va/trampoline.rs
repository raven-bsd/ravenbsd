// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! The one macro that emits every C-variadic trampoline: `raven_variadic!`.
//!
//! Derived from the project's own `libc/spike/va/src/asm.rs` (K03, same author, BSD-3) per
//! C59 section 5 row 2: the spike's 224-byte frame and `movups` become the 208-byte frame and
//! `movaps` of `donor-map/04` 3.9.1 (a `va_list` is 24 bytes, not 32), the target is the
//! public `v*` function and `N` is a macro parameter.
//!
//! One invocation emits exactly one stub for one exported name, in its own section so that
//! the static linker can drop it (`.text.<name>`): the stub spills the argument registers
//! into a save area, builds the platform's `va_list` at the offsets fixed by C59 section 4.4
//! and calls the Rust `v*` target with the named arguments untouched and `&va_list` in
//! argument register `N`.
//!
//! ```ignore
//! raven_variadic!(prefix: "", export: "printf", target: vprintf, named: 1);
//! raven_variadic!(prefix: "t_", export: "snprintf", target: v_snprintf, named: 3);
//! ```
//!
//! `named` is a `const` operand and is checked at compile time (`N <= 5` on x86_64, `N <= 7`
//! elsewhere). The target is a `sym` operand, so a typo is a compile error and never a link
//! error. The symbol name is `concat!($prefix, $export)`.
//!
//! On aarch64 and riscv64, until the reviewer sets `cfg(raven_va_verified_aarch64)` /
//! `cfg(raven_va_verified_riscv64)` after a passing `qemu-user` lane, the macro emits a
//! LINK-TIME FAILING stub: a single branch to `__raven_variadic_unsupported_<name>`, a symbol
//! that is never defined (H-16, RD-34). Only the `v*` forms work on those two architectures.

/// The register that carries `&va_list` in the x86_64 System V ABI: `rdi..r9`.
///
/// Private helper of [`raven_variadic!`] (`doc(hidden)`); `$crate`-addressed so that the
/// exported macro works from any crate that compiles this module.
#[doc(hidden)]
#[macro_export]
macro_rules! __raven_va_apreg_x86 {
    (0) => {
        "rdi"
    };
    (1) => {
        "rsi"
    };
    (2) => {
        "rdx"
    };
    (3) => {
        "rcx"
    };
    (4) => {
        "r8"
    };
    (5) => {
        "r9"
    };
}

/// The register that carries `&va_list` in the aarch64 AAPCS64 ABI: `x0..x7`.
#[doc(hidden)]
#[macro_export]
macro_rules! __raven_va_apreg_a64 {
    (0) => {
        "x0"
    };
    (1) => {
        "x1"
    };
    (2) => {
        "x2"
    };
    (3) => {
        "x3"
    };
    (4) => {
        "x4"
    };
    (5) => {
        "x5"
    };
    (6) => {
        "x6"
    };
    (7) => {
        "x7"
    };
}

/// The register that carries the riscv64 cursor: `a0..a7`.
#[doc(hidden)]
#[macro_export]
macro_rules! __raven_va_apreg_rv {
    (0) => {
        "a0"
    };
    (1) => {
        "a1"
    };
    (2) => {
        "a2"
    };
    (3) => {
        "a3"
    };
    (4) => {
        "a4"
    };
    (5) => {
        "a5"
    };
    (6) => {
        "a6"
    };
    (7) => {
        "a7"
    };
}

/// Emit one C-variadic trampoline for `concat!($prefix, $export)`.
///
/// The four fields are the row fields of `for_each_variadic!` (`crate::va::table`): `prefix`
/// and `export` build the symbol name, `target` is the Rust `v*` function the stub calls and
/// `named` is the number of fixed leading arguments.
///
/// The emitted stub never uses a callee-saved register other than `rbp`/`x29`/`ra`, and
/// returns the target's return value untouched.
#[macro_export]
macro_rules! raven_variadic {
    (prefix: $prefix:literal, export: $export:literal, target: $target:path, named: $n:tt $(,)?) => {
        // `va::MAX_NAMED_X86_64` (5) and `va::MAX_NAMED_OTHER` (7), spelled as literals so
        // that the macro also works from a bare `rustc` build of the module (the golden and
        // failing-stub tests) where the module's constants are not in scope.
        #[cfg(target_arch = "x86_64")]
        const _: () = assert!(
            $n <= 5,
            "x86_64 variadic stub: named arguments leave no register for &va_list"
        );
        #[cfg(not(target_arch = "x86_64"))]
        const _: () = assert!(
            $n <= 7,
            "variadic stub: named arguments leave no register for &va_list"
        );

        // x86_64 System V, hosted hard-float. Frame 208 = 13*16; entry `rsp % 16 == 8`, after
        // `push` 0, after `sub` 0, so the `call` is 16-byte aligned and `movaps` is legal.
        // `%al` is the caller's vector-register count: only spill when it is non-zero (T3).
        #[cfg(all(target_arch = "x86_64", not(raven_va_softfloat)))]
        $crate::__global_asm_x86! {
            prefix: $prefix,
            export: $export,
            target: $target,
            n: $n,
            fp: "48",
        }

        // x86_64 System V, soft-float profile (H-20, RD-new 1): no `movaps`, no `%al` test —
        // the object runs where SSE is disabled — and `fp_offset = 176` so that any float
        // register read falls through to the overflow area.
        #[cfg(all(target_arch = "x86_64", raven_va_softfloat))]
        $crate::__global_asm_x86! {
            prefix: $prefix,
            export: $export,
            target: $target,
            n: $n,
            fp: "176",
            softfloat: yes,
        }

        // aarch64 AAPCS64, unverified: link-time failing stub (H-16, RD-34).
        #[cfg(all(target_arch = "aarch64", not(raven_va_verified_aarch64)))]
        $crate::__global_asm_a64_failing! {
            prefix: $prefix,
            export: $export,
        }

        // aarch64 AAPCS64, hard-float and verified: both register groups are saved.
        #[cfg(all(
            target_arch = "aarch64",
            raven_va_verified_aarch64,
            not(any(target_abi = "softfloat", raven_va_softfloat))
        ))]
        $crate::__global_asm_a64! {
            prefix: $prefix,
            export: $export,
            target: $target,
            n: $n,
            vr: "-128",
        }

        // aarch64 AAPCS64, soft-float and verified: no VR area, `vr_offs = 0`.
        #[cfg(all(
            target_arch = "aarch64",
            raven_va_verified_aarch64,
            any(target_abi = "softfloat", raven_va_softfloat)
        ))]
        $crate::__global_asm_a64! {
            prefix: $prefix,
            export: $export,
            target: $target,
            n: $n,
            vr: "0",
            softfloat: yes,
        }

        // riscv64 psABI, unverified: link-time failing stub (H-16, RD-34).
        #[cfg(all(target_arch = "riscv64", not(raven_va_verified_riscv64)))]
        $crate::__global_asm_rv_failing! {
            prefix: $prefix,
            export: $export,
        }

        // riscv64 psABI, verified: `a0..a7` saved contiguously below the entry `sp`.
        #[cfg(all(target_arch = "riscv64", raven_va_verified_riscv64))]
        $crate::__global_asm_rv! {
            prefix: $prefix,
            export: $export,
            target: $target,
            n: $n,
        }
    };
}

/// The x86_64 body of [`raven_variadic!`] (private; `doc(hidden)`).
#[doc(hidden)]
#[macro_export]
macro_rules! __global_asm_x86 {
    (prefix: $prefix:literal, export: $export:literal, target: $target:path, n: $n:tt, fp: $fp:literal $(, softfloat: $softfloat:ident)? $(,)?) => {
        core::arch::global_asm!(
            concat!(
                ".pushsection .text.", $prefix, $export, ",\"ax\",@progbits\n",
                ".globl ", $prefix, $export, "\n",
                ".type ", $prefix, $export, ",@function\n",
                $prefix, $export, ":\n",
                "  .cfi_startproc\n",
                "  push %rbp\n",
                "  .cfi_def_cfa_offset 16\n",
                "  .cfi_offset %rbp, -16\n",
                "  mov %rsp, %rbp\n",
                "  .cfi_def_cfa_register %rbp\n",
                "  sub $208, %rsp\n",
                "  mov %rdi, 0(%rsp)\n",
                "  mov %rsi, 8(%rsp)\n",
                "  mov %rdx, 16(%rsp)\n",
                "  mov %rcx, 24(%rsp)\n",
                "  mov %r8, 32(%rsp)\n",
                "  mov %r9, 40(%rsp)\n",
                $crate::__x86_fp_spill!($($softfloat)?),
                "1:\n",
                "  movl ${gp}, 176(%rsp)\n",
                "  movl $", $fp, ", 180(%rsp)\n",
                "  lea 16(%rbp), %rax\n",
                "  mov %rax, 184(%rsp)\n",
                "  mov %rsp, 192(%rsp)\n",
                "  lea 176(%rsp), %", $crate::__raven_va_apreg_x86!($n), "\n",
                "  call {t}@PLT\n",
                "  leave\n",
                "  .cfi_def_cfa %rsp, 8\n",
                "  ret\n",
                "  .cfi_endproc\n",
                ".size ", $prefix, $export, ", .-", $prefix, $export, "\n",
                ".popsection\n",
            ),
            gp = const 8 * $n,
            t = sym $target,
            options(att_syntax),
        );
    };
}

/// The `%al` test and the XMM spill, or nothing on the soft-float profile.
#[doc(hidden)]
#[macro_export]
macro_rules! __x86_fp_spill {
    () => {
        concat!(
            "  test %al, %al\n",
            "  jz 1f\n",
            "  movaps %xmm0, 48(%rsp)\n",
            "  movaps %xmm1, 64(%rsp)\n",
            "  movaps %xmm2, 80(%rsp)\n",
            "  movaps %xmm3, 96(%rsp)\n",
            "  movaps %xmm4, 112(%rsp)\n",
            "  movaps %xmm5, 128(%rsp)\n",
            "  movaps %xmm6, 144(%rsp)\n",
            "  movaps %xmm7, 160(%rsp)\n",
        )
    };
    (yes) => {
        ""
    };
}

/// The aarch64 hard/soft-float body of [`raven_variadic!`] (private; `doc(hidden)`).
#[doc(hidden)]
#[macro_export]
macro_rules! __global_asm_a64 {
    (prefix: $prefix:literal, export: $export:literal, target: $target:path, n: $n:tt, vr: $vr:literal $(, softfloat: $softfloat:ident)? $(,)?) => {
        core::arch::global_asm!(
            concat!(
                ".pushsection .text.", $prefix, $export, ",\"ax\",%progbits\n",
                ".globl ", $prefix, $export, "\n",
                ".type ", $prefix, $export, ",%function\n",
                $prefix, $export, ":\n",
                "  .cfi_startproc\n",
                $crate::__a64_fp_spill!($($softfloat)?),
                "  stp x29, x30, [sp, #-240]!\n",
                "  mov x29, sp\n",
                "  stp x0, x1, [sp, #16]\n",
                "  stp x2, x3, [sp, #32]\n",
                "  stp x4, x5, [sp, #48]\n",
                "  stp x6, x7, [sp, #64]\n",
                "  add x9, sp, #240\n",
                "  str x9, [sp, #208]\n",
                "  add x9, sp, #80\n",
                "  str x9, [sp, #216]\n",
                "  add x9, sp, #208\n",
                "  str x9, [sp, #224]\n",
                "  mov w9, #{gron}\n",
                "  str w9, [sp, #232]\n",
                "  mov w9, #", $vr, "\n",
                "  str w9, [sp, #236]\n",
                "  add ", $crate::__raven_va_apreg_a64!($n), ", sp, #208\n",
                "  bl {t}\n",
                "  ldp x29, x30, [sp], #240\n",
                "  ret\n",
                "  .cfi_endproc\n",
                ".size ", $prefix, $export, ", .-", $prefix, $export, "\n",
                ".popsection\n",
            ),
            gron = const -(64 - 8 * $n),
            t = sym $target,
        );
    };
}

/// The VR spill of the hard-float aarch64 arm, or nothing on the soft-float arm.
#[doc(hidden)]
#[macro_export]
macro_rules! __a64_fp_spill {
    () => {
        concat!(
            "  .arch_extension fp\n",
            "  stp q0, q1, [sp, #80]\n",
            "  stp q2, q3, [sp, #112]\n",
            "  stp q4, q5, [sp, #144]\n",
            "  stp q6, q7, [sp, #176]\n",
        )
    };
    (yes) => {
        ""
    };
}

/// The aarch64 link-time failing stub (private; `doc(hidden)`).
#[doc(hidden)]
#[macro_export]
macro_rules! __global_asm_a64_failing {
    (prefix: $prefix:literal, export: $export:literal $(,)?) => {
        core::arch::global_asm!(concat!(
            ".pushsection .text.",
            $prefix,
            $export,
            ",\"ax\",%progbits\n",
            ".globl ",
            $prefix,
            $export,
            "\n",
            ".type ",
            $prefix,
            $export,
            ",%function\n",
            $prefix,
            $export,
            ":\n",
            "  b __raven_variadic_unsupported_",
            $prefix,
            $export,
            "\n",
            ".size ",
            $prefix,
            $export,
            ", .-",
            $prefix,
            $export,
            "\n",
            ".popsection\n",
        ),);
    };
}

/// The riscv64 body of [`raven_variadic!`] (private; `doc(hidden)`).
#[doc(hidden)]
#[macro_export]
macro_rules! __global_asm_rv {
    (prefix: $prefix:literal, export: $export:literal, target: $target:path, n: $n:tt $(,)?) => {
        core::arch::global_asm!(
            concat!(
                ".pushsection .text.", $prefix, $export, ",\"ax\",@progbits\n",
                ".globl ", $prefix, $export, "\n",
                ".type ", $prefix, $export, ",@function\n",
                $prefix, $export, ":\n",
                "  .cfi_startproc\n",
                "  addi sp, sp, -80\n",
                "  sd ra, 0(sp)\n",
                "  sd a0, 16(sp)\n",
                "  sd a1, 24(sp)\n",
                "  sd a2, 32(sp)\n",
                "  sd a3, 40(sp)\n",
                "  sd a4, 48(sp)\n",
                "  sd a5, 56(sp)\n",
                "  sd a6, 64(sp)\n",
                "  sd a7, 72(sp)\n",
                "  addi ", $crate::__raven_va_apreg_rv!($n), ", sp, {cur}\n",
                "  call {t}\n",
                "  ld ra, 0(sp)\n",
                "  addi sp, sp, 80\n",
                "  ret\n",
                "  .cfi_endproc\n",
                ".size ", $prefix, $export, ", .-", $prefix, $export, "\n",
                ".popsection\n",
            ),
            cur = const 16 + 8 * $n,
            t = sym $target,
        );
    };
}

/// The riscv64 link-time failing stub (private; `doc(hidden)`).
#[doc(hidden)]
#[macro_export]
macro_rules! __global_asm_rv_failing {
    (prefix: $prefix:literal, export: $export:literal $(,)?) => {
        core::arch::global_asm!(concat!(
            ".pushsection .text.",
            $prefix,
            $export,
            ",\"ax\",@progbits\n",
            ".globl ",
            $prefix,
            $export,
            "\n",
            ".type ",
            $prefix,
            $export,
            ",@function\n",
            $prefix,
            $export,
            ":\n",
            "  tail __raven_variadic_unsupported_",
            $prefix,
            $export,
            "\n",
            ".size ",
            $prefix,
            $export,
            ", .-",
            $prefix,
            $export,
            "\n",
            ".popsection\n",
        ),);
    };
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "trampoline_ut.rs"]
mod tests;
