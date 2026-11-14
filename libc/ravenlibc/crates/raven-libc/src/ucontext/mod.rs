// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public `getcontext`/`setcontext`/`swapcontext` entry points as tail-jumps over C58.

#[path = "makecontext.rs"]
mod makecontext;

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    r#"
    .text
    .globl getcontext
    .type getcontext,@function
getcontext:
    jmp __rl_getcontext
    .globl setcontext
    .type setcontext,@function
setcontext:
    jmp __rl_setcontext
    .globl swapcontext
    .type swapcontext,@function
swapcontext:
    jmp __rl_swapcontext
"#,
    options(att_syntax)
);
#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    r#"
    .text
    .globl getcontext
    .type getcontext,%function
getcontext:
    b __rl_getcontext
    .globl setcontext
    .type setcontext,%function
setcontext:
    b __rl_setcontext
    .globl swapcontext
    .type swapcontext,%function
swapcontext:
    b __rl_swapcontext
"#
);
#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(
    r#"
    .text
    .globl getcontext
    .type getcontext,@function
getcontext:
    tail __rl_getcontext
    .globl setcontext
    .type setcontext,@function
setcontext:
    tail __rl_setcontext
    .globl swapcontext
    .type swapcontext,@function
swapcontext:
    tail __rl_swapcontext
"#
);
