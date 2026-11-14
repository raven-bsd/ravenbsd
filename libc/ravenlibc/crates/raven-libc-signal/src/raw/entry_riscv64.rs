// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RISC-V signal entry stub and `SIGRETURN` (CLEANROOM, PROPOSED PG-7).

core::arch::global_asm!(
    r#"
    .text
    .global __raven_sigentry
    .hidden __raven_sigentry
    .type __raven_sigentry,%function
__raven_sigentry:
    andi sp, sp, -16
    lui t0, %hi(__rl_entry_fn)
    addi t0, t0, %lo(__rl_entry_fn)
    ld t0, 0(t0)
    beqz t0, 1f
    jalr ra, 0(t0)
1:
    j __rl_sigreturn_stub

    .global __rl_sigreturn_stub
    .hidden __rl_sigreturn_stub
    .type __rl_sigreturn_stub,%function
__rl_sigreturn_stub:
    li a7, 0x82
    ecall
    ebreak

    .data
    .global __rl_entry_fn
    .hidden __rl_entry_fn
    .align 3
__rl_entry_fn:
    .quad 0
"#
);
