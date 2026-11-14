// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! x86-64 `getcontext`/`setcontext`/`swapcontext`/`makecontext` asm (CLEANROOM).
//!
//! POSIX specifies these; no permissive donor file exists (NetBSD's `swapcontext.S` is BSD-4).
//! The saved image is the Linux `mcontext_t`: `gregs[RBX,RBP,R12..R15,RSP,RIP]`, argument
//! registers zeroed by `getcontext`, the legacy FP area pointer at `mcontext+184`, MXCSR and the
//! x87 control word in the 512-byte `__fpregs_mem` at `ucontext+424`.

core::arch::global_asm!(
    r#"
    .text
    .global __rl_getcontext
    .hidden __rl_getcontext
    .type __rl_getcontext,@function
__rl_getcontext:
    mov %rbx,88(%rdi)
    mov %rbp,80(%rdi)
    mov %r12,32(%rdi)
    mov %r13,40(%rdi)
    mov %r14,48(%rdi)
    mov %r15,56(%rdi)
    lea 8(%rsp),%rax
    mov %rax,120(%rdi)
    mov (%rsp),%rax
    mov %rax,128(%rdi)
    movq $0,0(%rdi)
    movq $0,8(%rdi)
    movq $0,64(%rdi)
    movq $0,72(%rdi)
    movq $0,96(%rdi)
    movq $0,104(%rdi)
    movq $0,112(%rdi)
    stmxcsr 424(%rdi)
    fnstcw 428(%rdi)
    lea 424(%rdi),%rax
    mov %rax,224(%rdi)
    xor %eax,%eax
    ret

    .global __rl_setcontext
    .hidden __rl_setcontext
    .type __rl_setcontext,@function
__rl_setcontext:
    ldmxcsr 424(%rdi)
    fldcw 428(%rdi)
    mov %rdi,%r10
    mov 64(%r10),%rdi
    mov 72(%r10),%rsi
    mov 96(%r10),%rdx
    mov 112(%r10),%rcx
    mov 0(%r10),%r8
    mov 8(%r10),%r9
    mov 80(%r10),%rbp
    mov 88(%r10),%rbx
    mov 32(%r10),%r12
    mov 40(%r10),%r13
    mov 48(%r10),%r14
    mov 56(%r10),%r15
    mov 120(%r10),%rsp
    xor %eax,%eax
    jmp *128(%r10)

    .global __rl_swapcontext
    .hidden __rl_swapcontext
    .type __rl_swapcontext,@function
__rl_swapcontext:
    push %r12
    mov %rsi,%r12
    call __rl_getcontext
    lea 2f(%rip),%rax
    mov %rax,128(%rdi)
    mov %r12,%rdi
    call __rl_setcontext
2:
    pop %r12
    xor %eax,%eax
    ret

    .global __rl_start_context
    .hidden __rl_start_context
    .type __rl_start_context,@function
__rl_start_context:
    and $-16,%rsp
    call *%r12
    test %r13,%r13
    jz 1f
    lea 40(%r13),%rdi
    call __rl_setcontext
1:
    xor %edi,%edi
    call exit@PLT
    ud2

    .global __rl_call_handler
    .hidden __rl_call_handler
    .type __rl_call_handler,@function
__rl_call_handler:
    push %r12
    push %r13
    mov %rsp,%r12
    mov %rdi,%r13
    mov %rsi,%rax
    mov %rdx,%rdi
    mov %rcx,%rsi
    mov %r8,%rdx
    mov %r13,%rsp
    and $-16,%rsp
    call *%rax
    mov %r12,%rsp
    pop %r13
    pop %r12
    ret
"#,
    options(att_syntax)
);
