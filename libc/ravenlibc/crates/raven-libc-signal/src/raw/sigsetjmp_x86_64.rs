// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/x86_64/sigsetjmp.s @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor file has no per-file header; covered by musl/COPYRIGHT
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

//! x86-64 `sigsetjmp` and its tail, re-based onto the 392-byte `jmp_buf`.
//!
//! `__fl` is at 256 and `__ss` at 264 (`__ss[0]` mask, `__ss[1]` scratch, `__ss[2]` our mask
//! pointer). musl's offsets 64/72+8 are replaced; the tail is asm because it runs after the jump.

core::arch::global_asm!(
    r#"
    .text
    .global __rl_sigsetjmp
    .hidden __rl_sigsetjmp
    .type __rl_sigsetjmp,@function
__rl_sigsetjmp:
    test %esi,%esi
    jz 1f
    popq 256(%rdi)
    mov %rbx,272(%rdi)
    mov %rdx,280(%rdi)
    mov %rdi,%rbx
    call __rl_setjmp
    pushq 256(%rbx)
    mov %rbx,%rdi
    mov %eax,%esi
    mov 280(%rbx),%rdx
    mov 272(%rbx),%rbx
    jmp __rl_sigsetjmp_tail
1:
    jmp __rl_setjmp

    .global __rl_sigsetjmp_tail
    .hidden __rl_sigsetjmp_tail
    .type __rl_sigsetjmp_tail,@function
__rl_sigsetjmp_tail:
    test %esi,%esi
    jz 2f
    mov 264(%rdi),%rax
    mov %rax,(%rdx)
    jmp 3f
2:
    mov (%rdx),%rax
    mov %rax,264(%rdi)
3:
    mov %esi,%eax
    ret
"#,
    options(att_syntax)
);
