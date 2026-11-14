// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
use super::sigmask_ptr;
#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(r#"
.text
.globl sigsetjmp
.type sigsetjmp,@function
sigsetjmp:
    push %rdi
    push %rsi
    call {mask}
    mov %rax,%rdx
    pop %rsi
    pop %rdi
    jmp __rl_sigsetjmp
.globl __sigsetjmp
.set __sigsetjmp, sigsetjmp
"#, mask = sym sigmask_ptr, options(att_syntax));
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(r#"
.text
.globl sigsetjmp
.type sigsetjmp,%function
sigsetjmp:
    stp x0, x1, [sp, #-16]!
    bl {mask}
    mov x2, x0
    ldp x0, x1, [sp], #16
    b __rl_sigsetjmp
.globl __sigsetjmp
.set __sigsetjmp, sigsetjmp
"#, mask = sym sigmask_ptr);
#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(r#"
.text
.globl sigsetjmp
.type sigsetjmp,@function
sigsetjmp:
    addi sp, sp, -16
    sd a0, 0(sp)
    sd a1, 8(sp)
    call {mask}
    mv a2, a0
    ld a0, 0(sp)
    ld a1, 8(sp)
    addi sp, sp, 16
    tail __rl_sigsetjmp
.globl __sigsetjmp
.set __sigsetjmp, sigsetjmp
"#, mask = sym sigmask_ptr);
