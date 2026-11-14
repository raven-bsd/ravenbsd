// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! AArch64 signal entry stub and `SIGRETURN` (CLEANROOM, PROPOSED PG-7).

core::arch::global_asm!(
    r#"
    .text
    .global __raven_sigentry
    .hidden __raven_sigentry
    .type __raven_sigentry,%function
__raven_sigentry:
    mov x10, sp
    and x10, x10, #0xfffffffffffffff0
    mov sp, x10
    adrp x9, __rl_entry_fn
    ldr x9, [x9, :lo12:__rl_entry_fn]
    cbz x9, 1f
    blr x9
1:
    b __rl_sigreturn_stub

    .global __rl_sigreturn_stub
    .hidden __rl_sigreturn_stub
    .type __rl_sigreturn_stub,%function
__rl_sigreturn_stub:
    mov x8, #0x82
    svc #0
    brk #0

    .data
    .global __rl_entry_fn
    .hidden __rl_entry_fn
    .align 3
__rl_entry_fn:
    .quad 0
"#
);
