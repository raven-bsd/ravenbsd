// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/setjmp/aarch64/setjmp.s @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/setjmp/aarch64/longjmp.s @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor files have no per-file header; covered by musl/COPYRIGHT
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

//! AArch64 `setjmp`/`longjmp`; `d8-d15` only in the hard-float profile (H-20).
//!
//! Slots: `x19..x30` words 0..11, `sp` at offset 104, `d8..d15` at 112..168. The buffer stays
//! 392 bytes in both profiles; the soft-float build leaves the FP slots zero.

#[cfg(all(not(raven_va_softfloat), target_feature = "fp-armv8"))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_setjmp
    .hidden __rl_setjmp
    .type __rl_setjmp,%function
__rl_setjmp:
    stp x19, x20, [x0,#0]
    stp x21, x22, [x0,#16]
    stp x23, x24, [x0,#32]
    stp x25, x26, [x0,#48]
    stp x27, x28, [x0,#64]
    stp x29, x30, [x0,#80]
    mov x2, sp
    str x2, [x0,#104]
    stp d8, d9, [x0,#112]
    stp d10, d11, [x0,#128]
    stp d12, d13, [x0,#144]
    stp d14, d15, [x0,#160]
    mov x0, #0
    ret

    .global __rl_longjmp
    .hidden __rl_longjmp
    .type __rl_longjmp,%function
__rl_longjmp:
    ldp x19, x20, [x0,#0]
    ldp x21, x22, [x0,#16]
    ldp x23, x24, [x0,#32]
    ldp x25, x26, [x0,#48]
    ldp x27, x28, [x0,#64]
    ldp x29, x30, [x0,#80]
    ldr x2, [x0,#104]
    mov sp, x2
    ldp d8, d9, [x0,#112]
    ldp d10, d11, [x0,#128]
    ldp d12, d13, [x0,#144]
    ldp d14, d15, [x0,#160]
    cmp w1, 0
    csinc w0, w1, wzr, ne
    br x30

    .global __rl_current_sp
    .hidden __rl_current_sp
    .type __rl_current_sp,%function
__rl_current_sp:
    mov x0, sp
    ret
"#
);

#[cfg(any(raven_va_softfloat, not(target_feature = "fp-armv8")))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_setjmp
    .hidden __rl_setjmp
    .type __rl_setjmp,%function
__rl_setjmp:
    stp x19, x20, [x0,#0]
    stp x21, x22, [x0,#16]
    stp x23, x24, [x0,#32]
    stp x25, x26, [x0,#48]
    stp x27, x28, [x0,#64]
    stp x29, x30, [x0,#80]
    mov x2, sp
    str x2, [x0,#104]
    mov x0, #0
    ret

    .global __rl_longjmp
    .hidden __rl_longjmp
    .type __rl_longjmp,%function
__rl_longjmp:
    ldp x19, x20, [x0,#0]
    ldp x21, x22, [x0,#16]
    ldp x23, x24, [x0,#32]
    ldp x25, x26, [x0,#48]
    ldp x27, x28, [x0,#64]
    ldp x29, x30, [x0,#80]
    ldr x2, [x0,#104]
    mov sp, x2
    cmp w1, 0
    csinc w0, w1, wzr, ne
    br x30

    .global __rl_current_sp
    .hidden __rl_current_sp
    .type __rl_current_sp,%function
__rl_current_sp:
    mov x0, sp
    ret
"#
);
