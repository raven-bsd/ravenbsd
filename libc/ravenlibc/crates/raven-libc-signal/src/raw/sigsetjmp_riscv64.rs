// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/riscv64/sigsetjmp.s @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! RISC-V `sigsetjmp` and tail, re-based onto `__fl` 256 / `__ss` 264 / mask pointer 280.

core::arch::global_asm!(
    r#"
    .text
    .global __rl_sigsetjmp
    .hidden __rl_sigsetjmp
    .type __rl_sigsetjmp,%function
__rl_sigsetjmp:
    bnez a1, 1f
    tail __rl_setjmp
1:
    sd ra, 256(a0)
    sd s0, 272(a0)
    sd a2, 280(a0)
    mv s0, a0
    call __rl_setjmp
    mv a1, a0
    mv a0, s0
    ld a2, 280(a0)
    ld ra, 256(a0)
    ld s0, 272(a0)
    tail __rl_sigsetjmp_tail

    .global __rl_sigsetjmp_tail
    .hidden __rl_sigsetjmp_tail
    .type __rl_sigsetjmp_tail,%function
__rl_sigsetjmp_tail:
    beqz a1, 2f
    ld a3, 264(a0)
    sd a3, 0(a2)
    j 3f
2:
    ld a3, 0(a2)
    sd a3, 264(a0)
3:
    mv a0, a1
    ret
"#
);
