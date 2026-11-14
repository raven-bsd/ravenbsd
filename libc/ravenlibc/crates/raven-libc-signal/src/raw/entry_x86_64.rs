// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 signal entry stub and `SIGRETURN` (CLEANROOM, PROPOSED PG-7).
//!
//! The kernel resumes at `__raven_sigentry` with the kick bits in `rdi` and the optional fault
//! record pointer in `rsi`, on a stack of `interrupted_sp - 128`. The stub reads the entry
//! function cell set by the shell (`__rl_entry_fn`), calls it and issues `SIGRETURN` (0x82).
//! `SIGRETURN` is the only invented number and is the labelled PROPOSED PG-7 value.

core::arch::global_asm!(
    r#"
    .text
    .global __raven_sigentry
    .hidden __raven_sigentry
    .type __raven_sigentry,@function
__raven_sigentry:
    and $-16,%rsp
    mov __rl_entry_fn(%rip),%rax
    test %rax,%rax
    jz 1f
    call *%rax
1:
    jmp __rl_sigreturn_stub

    .global __rl_sigreturn_stub
    .hidden __rl_sigreturn_stub
    .type __rl_sigreturn_stub,@function
__rl_sigreturn_stub:
    mov $0x82,%eax
    syscall
    ud2

    .data
    .global __rl_entry_fn
    .hidden __rl_entry_fn
    .align 8
__rl_entry_fn:
    .quad 0
"#,
    options(att_syntax)
);
