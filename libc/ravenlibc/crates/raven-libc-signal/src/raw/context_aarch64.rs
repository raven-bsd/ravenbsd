// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! AArch64 `getcontext`/`setcontext`/`swapcontext`/`makecontext` asm (CLEANROOM).
//!
//! The machine context is the Linux AArch64 `mcontext_t`: fault address at 0, `regs[0..30]` at
//! words 1..31, `sp` at word 32, `pc` at 33, `pstate` at 34, then the 4096-byte reserved area.
//! This crate keeps `d8-d15` at 280..343, `FPCR` at 344 and `FPSR` at 348 (hard-float profile);
//! the soft-float profile leaves the slots zero but keeps the same sizes. `x19` holds the
//! `ucontext` pointer in a `makecontext` function, `x20` the function and `x21` `uc_link`.

#[cfg(all(not(raven_va_softfloat), target_feature = "fp-armv8"))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_getcontext
    .hidden __rl_getcontext
    .type __rl_getcontext,%function
__rl_getcontext:
    stp x19, x20, [x0,#160]
    stp x21, x22, [x0,#176]
    stp x23, x24, [x0,#192]
    stp x25, x26, [x0,#208]
    stp x27, x28, [x0,#224]
    stp x29, x30, [x0,#240]
    mov x2, sp
    str x2, [x0,#256]
    str x30, [x0,#264]
    mrs x1, nzcv
    str x1, [x0,#272]
    stp xzr, xzr, [x0,#8]
    stp xzr, xzr, [x0,#24]
    stp xzr, xzr, [x0,#40]
    stp xzr, xzr, [x0,#56]
    stp d8, d9, [x0,#280]
    stp d10, d11, [x0,#296]
    stp d12, d13, [x0,#312]
    stp d14, d15, [x0,#328]
    mrs x1, fpcr
    str w1, [x0,#344]
    mrs x2, fpsr
    str w2, [x0,#348]
    mov w0, #0
    ret

    .global __rl_setcontext
    .hidden __rl_setcontext
    .type __rl_setcontext,%function
__rl_setcontext:
    ldp d8, d9, [x0,#280]
    ldp d10, d11, [x0,#296]
    ldp d12, d13, [x0,#312]
    ldp d14, d15, [x0,#328]
    ldr w1, [x0,#344]
    msr fpcr, x1
    ldr w2, [x0,#348]
    msr fpsr, x2
    mov x9, x0
    ldp x0, x1, [x9,#8]
    ldp x2, x3, [x9,#24]
    ldp x4, x5, [x9,#40]
    ldp x6, x7, [x9,#56]
    ldp x19, x20, [x9,#160]
    ldp x21, x22, [x9,#176]
    ldp x23, x24, [x9,#192]
    ldp x25, x26, [x9,#208]
    ldp x27, x28, [x9,#224]
    ldp x29, x30, [x9,#240]
    ldr x10, [x9,#256]
    mov sp, x10
    ldr x10, [x9,#264]
    ldr x11, [x9,#272]
    msr nzcv, x11
    br x10

    .global __rl_swapcontext
    .hidden __rl_swapcontext
    .type __rl_swapcontext,%function
__rl_swapcontext:
    mov x19, x30
    mov x20, x1
    mov x21, x0
    bl __rl_getcontext
    adr x10, 2f
    str x10, [x21,#264]
    mov x0, x20
    b __rl_setcontext
2:
    mov x30, x19
    mov w0, #0
    ret

"#
);

#[cfg(any(raven_va_softfloat, not(target_feature = "fp-armv8")))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_getcontext
    .hidden __rl_getcontext
    .type __rl_getcontext,%function
__rl_getcontext:
    stp x19, x20, [x0,#160]
    stp x21, x22, [x0,#176]
    stp x23, x24, [x0,#192]
    stp x25, x26, [x0,#208]
    stp x27, x28, [x0,#224]
    stp x29, x30, [x0,#240]
    mov x2, sp
    str x2, [x0,#256]
    str x30, [x0,#264]
    mrs x1, nzcv
    str x1, [x0,#272]
    stp xzr, xzr, [x0,#8]
    stp xzr, xzr, [x0,#24]
    stp xzr, xzr, [x0,#40]
    stp xzr, xzr, [x0,#56]
    mov w0, #0
    ret

    .global __rl_setcontext
    .hidden __rl_setcontext
    .type __rl_setcontext,%function
__rl_setcontext:
    mov x9, x0
    ldp x0, x1, [x9,#8]
    ldp x2, x3, [x9,#24]
    ldp x4, x5, [x9,#40]
    ldp x6, x7, [x9,#56]
    ldp x19, x20, [x9,#160]
    ldp x21, x22, [x9,#176]
    ldp x23, x24, [x9,#192]
    ldp x25, x26, [x9,#208]
    ldp x27, x28, [x9,#224]
    ldp x29, x30, [x9,#240]
    ldr x10, [x9,#256]
    mov sp, x10
    ldr x10, [x9,#264]
    ldr x11, [x9,#272]
    msr nzcv, x11
    br x10

    .global __rl_swapcontext
    .hidden __rl_swapcontext
    .type __rl_swapcontext,%function
__rl_swapcontext:
    mov x19, x30
    mov x20, x1
    mov x21, x0
    bl __rl_getcontext
    adr x10, 2f
    str x10, [x21,#264]
    mov x0, x20
    b __rl_setcontext
2:
    mov x30, x19
    mov w0, #0
    ret

"#
);
