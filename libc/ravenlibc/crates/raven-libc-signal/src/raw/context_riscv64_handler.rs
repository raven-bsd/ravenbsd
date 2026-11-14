// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RISC-V `makecontext` target and signal-handler call helpers (CLEANROOM).
//!
//! Split from `context_riscv64.rs` so each archive member defines at most four global
//! text symbols (C62b K4). `s0`/`s1`/`s2` carry the context pointer, function and
//! `uc_link` set by `makecontext`.

#[cfg(all(not(raven_va_softfloat), target_feature = "d"))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_start_context
    .hidden __rl_start_context
    .type __rl_start_context,%function
__rl_start_context:
    andi sp, sp, -16
    jalr ra, 0(s1)
    beqz s2, 1f
    addi a0, s2, 176
    call __rl_setcontext
1:
    li a0, 0
    call exit
    ebreak

    .global __rl_call_handler
    .hidden __rl_call_handler
    .type __rl_call_handler,%function
__rl_call_handler:
    addi sp, sp, -16
    sd s0, 0(sp)
    sd s1, 8(sp)
    mv s1, sp
    mv s0, a1
    mv t0, a0
    mv a0, a2
    mv a1, a3
    mv a2, a4
    mv sp, t0
    andi sp, sp, -16
    jalr ra, 0(s0)
    mv sp, s1
    ld s0, 0(sp)
    ld s1, 8(sp)
    addi sp, sp, 16
    ret
"#
);

#[cfg(any(raven_va_softfloat, not(target_feature = "d")))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_start_context
    .hidden __rl_start_context
    .type __rl_start_context,%function
__rl_start_context:
    andi sp, sp, -16
    jalr ra, 0(s1)
    beqz s2, 1f
    addi a0, s2, 176
    call __rl_setcontext
1:
    li a0, 0
    call exit
    ebreak

    .global __rl_call_handler
    .hidden __rl_call_handler
    .type __rl_call_handler,%function
__rl_call_handler:
    addi sp, sp, -16
    sd s0, 0(sp)
    sd s1, 8(sp)
    mv s1, sp
    mv s0, a1
    mv t0, a0
    mv a0, a2
    mv a1, a3
    mv a2, a4
    mv sp, t0
    andi sp, sp, -16
    jalr ra, 0(s0)
    mv sp, s1
    ld s0, 0(sp)
    ld s1, 8(sp)
    addi sp, sp, 16
    ret
"#
);
