// SPDX-License-Identifier: MIT
// Copyright 2011-2012 Nicholas J. Kain, licensed under standard MIT license
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/setjmp/x86_64/setjmp.s @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/setjmp/x86_64/longjmp.s @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! x86-64 `setjmp`/`longjmp` (Kain's two musl files merged into one hidden symbol pair).
//!
//! `jmp_buf` words 0..7 are `rbx rbp r12 r13 r14 r15 rsp rip`; the buffer is 392 bytes but only
//! the first eight words are touched. `longjmp(buf, 0)` returns 1.

core::arch::global_asm!(
    r#"
    .text
    .global __rl_setjmp
    .hidden __rl_setjmp
    .type __rl_setjmp,@function
__rl_setjmp:
    mov %rbx,0(%rdi)
    mov %rbp,8(%rdi)
    mov %r12,16(%rdi)
    mov %r13,24(%rdi)
    mov %r14,32(%rdi)
    mov %r15,40(%rdi)
    lea 8(%rsp),%rdx
    mov %rdx,48(%rdi)
    mov (%rsp),%rdx
    mov %rdx,56(%rdi)
    xor %eax,%eax
    ret

    .global __rl_longjmp
    .hidden __rl_longjmp
    .type __rl_longjmp,@function
__rl_longjmp:
    xor %eax,%eax
    cmp $1,%esi
    adc %esi,%eax
    mov 0(%rdi),%rbx
    mov 8(%rdi),%rbp
    mov 16(%rdi),%r12
    mov 24(%rdi),%r13
    mov 32(%rdi),%r14
    mov 40(%rdi),%r15
    mov 48(%rdi),%rsp
    jmp *56(%rdi)

    .global __rl_current_sp
    .hidden __rl_current_sp
    .type __rl_current_sp,@function
__rl_current_sp:
    mov %rsp,%rax
    ret
"#,
    options(att_syntax)
);
