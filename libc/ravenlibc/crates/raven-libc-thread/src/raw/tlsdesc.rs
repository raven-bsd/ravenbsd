// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from musl:src/ldso/x86_64/tlsdesc.s, src/ldso/aarch64/tlsdesc.s, src/ldso/riscv64/tlsdesc.s @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor files have no per-file header; covered by musl/COPYRIGHT
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! The TLS descriptor entries, `__rl_tlsdesc_static` and `__rl_tlsdesc_dynamic`.
//!
//! The symbols are hidden; the shell exports `__tlsdesc_static`/`__tlsdesc_dynamic` as aliases
//! (RD-new 5, L-2). `static` returns the precomputed tp-relative offset at `descriptor + 8`.
//! `dynamic` reads the `{module, offset}` record at `descriptor + 8`, adds `dtv[module]` and
//! subtracts `tp`. The ABI preserves every register except the result (`rax`/`x0`/`a0`), so the
//! push/pop pairs are transcribed rather than optimised.

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .text
    .globl __rl_tlsdesc_static
    .hidden __rl_tlsdesc_static
    .type __rl_tlsdesc_static,@function
__rl_tlsdesc_static:
    mov 8(%rax), %rax
    ret
    .size __rl_tlsdesc_static, . - __rl_tlsdesc_static

    .globl __rl_tlsdesc_dynamic
    .hidden __rl_tlsdesc_dynamic
    .type __rl_tlsdesc_dynamic,@function
__rl_tlsdesc_dynamic:
    mov 8(%rax), %rax
    push %rdx
    mov %fs:8, %rdx
    push %rcx
    mov (%rax), %rcx
    mov 8(%rax), %rax
    add (%rdx,%rcx,8), %rax
    pop %rcx
    sub %fs:0, %rax
    pop %rdx
    ret
    .size __rl_tlsdesc_dynamic, . - __rl_tlsdesc_dynamic
"#,
    options(att_syntax)
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .text
    .globl __rl_tlsdesc_static
    .hidden __rl_tlsdesc_static
    .type __rl_tlsdesc_static,%function
__rl_tlsdesc_static:
    ldr x0, [x0, #8]
    ret
    .size __rl_tlsdesc_static, . - __rl_tlsdesc_static

    .globl __rl_tlsdesc_dynamic
    .hidden __rl_tlsdesc_dynamic
    .type __rl_tlsdesc_dynamic,%function
__rl_tlsdesc_dynamic:
    stp x1, x2, [sp, #-16]!
    mrs x1, tpidr_el0
    ldr x0, [x0, #8]
    ldp x0, x2, [x0]
    sub x2, x2, x1
    ldr x1, [x1, #-8]
    ldr x1, [x1, x0, lsl #3]
    add x0, x1, x2
    ldp x1, x2, [sp], #16
    ret
    .size __rl_tlsdesc_dynamic, . - __rl_tlsdesc_dynamic
"#
);

#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(
    r#"
    .text
    .globl __rl_tlsdesc_static
    .hidden __rl_tlsdesc_static
    .type __rl_tlsdesc_static,%function
__rl_tlsdesc_static:
    ld a0, 8(a0)
    jr t0
    .size __rl_tlsdesc_static, . - __rl_tlsdesc_static

    .globl __rl_tlsdesc_dynamic
    .hidden __rl_tlsdesc_dynamic
    .type __rl_tlsdesc_dynamic,%function
__rl_tlsdesc_dynamic:
    add sp, sp, -16
    sd t1, (sp)
    sd t2, 8(sp)
    ld t2, -8(tp)
    ld a0, 8(a0)
    ld t1, 8(a0)
    ld a0, (a0)
    sll a0, a0, 3
    add a0, a0, t2
    ld a0, (a0)
    add a0, a0, t1
    sub a0, a0, tp
    ld t1, (sp)
    ld t2, 8(sp)
    add sp, sp, 16
    jr t0
    .size __rl_tlsdesc_dynamic, . - __rl_tlsdesc_dynamic
"#
);

unsafe extern "C" {
    fn __rl_tlsdesc_static(descriptor: *mut usize) -> usize;
    fn __rl_tlsdesc_dynamic(descriptor: *mut usize) -> usize;
}

/// Call the static TLS descriptor entry.
///
/// The ABI passes the descriptor in `%rax`/`x0`/`a0`, not in the first argument register, so the
/// wrapper loads that register explicitly.
///
/// # Safety
///
/// `descriptor` must be a live TLS descriptor whose second word is the precomputed offset.
pub unsafe fn static_entry(descriptor: *mut usize) -> usize {
    let result: usize;
    // SAFETY: the caller guarantees the descriptor; the asm only reads word 1 and the symbol
    // preserves every register except the result.
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "call {}",
            sym __rl_tlsdesc_static,
            inout("rax") descriptor as usize => result,
        );
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!(
            "bl {}",
            sym __rl_tlsdesc_static,
            inout("x0") descriptor as usize => result,
        );
        #[cfg(target_arch = "riscv64")]
        core::arch::asm!(
            "call {}",
            sym __rl_tlsdesc_static,
            inout("a0") descriptor as usize => result,
        );
    }
    result
}

/// Call the dynamic TLS descriptor entry.
///
/// # Safety
///
/// `descriptor` must be a live TLS descriptor and the thread pointer must address a live DTV.
pub unsafe fn dynamic_entry(descriptor: *mut usize) -> usize {
    let result: usize;
    // SAFETY: the caller guarantees the descriptor and the thread pointer; the asm saves the
    // registers it uses.
    unsafe {
        #[cfg(target_arch = "x86_64")]
        core::arch::asm!(
            "call {}",
            sym __rl_tlsdesc_dynamic,
            inout("rax") descriptor as usize => result,
        );
        #[cfg(target_arch = "aarch64")]
        core::arch::asm!(
            "bl {}",
            sym __rl_tlsdesc_dynamic,
            inout("x0") descriptor as usize => result,
        );
        #[cfg(target_arch = "riscv64")]
        core::arch::asm!(
            "call {}",
            sym __rl_tlsdesc_dynamic,
            inout("a0") descriptor as usize => result,
        );
    }
    result
}

#[cfg(test)]
#[path = "tlsdesc_ut.rs"]
mod tests;
