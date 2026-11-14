// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/aarch64/sigsetjmp.s @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! AArch64 `sigsetjmp` and tail, re-based onto `__fl` 256 / `__ss` 264 / mask pointer 280.

core::arch::global_asm!(
    r#"
    .text
    .global __rl_sigsetjmp
    .hidden __rl_sigsetjmp
    .type __rl_sigsetjmp,%function
__rl_sigsetjmp:
    cbz x1, 1f
    str x30, [x0,#256]
    str x19, [x0,#272]
    str x2, [x0,#280]
    mov x19, x0
    bl __rl_setjmp
    mov w1, w0
    mov x0, x19
    ldr x2, [x0,#280]
    ldr x30, [x0,#256]
    ldr x19, [x0,#272]
    b __rl_sigsetjmp_tail
1:
    b __rl_setjmp

    .global __rl_sigsetjmp_tail
    .hidden __rl_sigsetjmp_tail
    .type __rl_sigsetjmp_tail,%function
__rl_sigsetjmp_tail:
    cbz w1, 2f
    ldr x3, [x0,#264]
    str x3, [x2]
    b 3f
2:
    ldr x3, [x2]
    str x3, [x0,#264]
3:
    mov w0, w1
    ret
"#
);
