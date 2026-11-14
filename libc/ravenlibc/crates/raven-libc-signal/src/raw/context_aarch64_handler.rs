// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! AArch64 `makecontext` target and signal-handler call helpers (CLEANROOM).
//!
//! Split from `context_aarch64.rs` so each archive member defines at most four global
//! text symbols (C62b K4). `x19`/`x20`/`x21` carry the context pointer, function and
//! `uc_link` set by `makecontext`.

#[cfg(all(not(raven_va_softfloat), target_feature = "fp-armv8"))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_start_context
    .hidden __rl_start_context
    .type __rl_start_context,%function
__rl_start_context:
    mov x9, sp
    and x9, x9, #0xfffffffffffffff0
    mov sp, x9
    blr x20
    cbz x21, 1f
    add x0, x21, #176
    bl __rl_setcontext
1:
    mov w0, #0
    bl exit
    brk #0

    .global __rl_call_handler
    .hidden __rl_call_handler
    .type __rl_call_handler,%function
__rl_call_handler:
    stp x19, x20, [sp,#-16]!
    mov x20, sp
    mov x19, x1
    mov x9, x0
    mov x0, x2
    mov x1, x3
    mov x2, x4
    mov sp, x9
    mov x10, sp
    and x10, x10, #0xfffffffffffffff0
    mov sp, x10
    blr x19
    mov sp, x20
    ldp x19, x20, [sp],#16
    ret
"#
);

#[cfg(any(raven_va_softfloat, not(target_feature = "fp-armv8")))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_start_context
    .hidden __rl_start_context
    .type __rl_start_context,%function
__rl_start_context:
    mov x9, sp
    and x9, x9, #0xfffffffffffffff0
    mov sp, x9
    blr x20
    cbz x21, 1f
    add x0, x21, #176
    bl __rl_setcontext
1:
    mov w0, #0
    bl exit
    brk #0

    .global __rl_call_handler
    .hidden __rl_call_handler
    .type __rl_call_handler,%function
__rl_call_handler:
    stp x19, x20, [sp,#-16]!
    mov x20, sp
    mov x19, x1
    mov x9, x0
    mov x0, x2
    mov x1, x3
    mov x2, x4
    mov sp, x9
    mov x10, sp
    and x10, x10, #0xfffffffffffffff0
    mov sp, x10
    blr x19
    mov sp, x20
    ldp x19, x20, [sp],#16
    ret
"#
);
