// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/setjmp/riscv64/setjmp.S @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/setjmp/riscv64/longjmp.S @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! RISC-V `setjmp`/`longjmp`; `fs0-fs11` only in the hard-float profile.
//!
//! The donor's `FSX`/`FLX` preprocessor macros become the `raven_va_softfloat` cfg. Slots:
//! `s0..s11` words 0..11, `sp` offset 96, `ra` offset 104, `fs0..fs11` 112..200.

#[cfg(all(not(raven_va_softfloat), target_feature = "d"))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_setjmp
    .hidden __rl_setjmp
    .type __rl_setjmp,%function
__rl_setjmp:
    sd s0, 0(a0)
    sd s1, 8(a0)
    sd s2, 16(a0)
    sd s3, 24(a0)
    sd s4, 32(a0)
    sd s5, 40(a0)
    sd s6, 48(a0)
    sd s7, 56(a0)
    sd s8, 64(a0)
    sd s9, 72(a0)
    sd s10, 80(a0)
    sd s11, 88(a0)
    sd sp, 96(a0)
    sd ra, 104(a0)
    fsd fs0, 112(a0)
    fsd fs1, 120(a0)
    fsd fs2, 128(a0)
    fsd fs3, 136(a0)
    fsd fs4, 144(a0)
    fsd fs5, 152(a0)
    fsd fs6, 160(a0)
    fsd fs7, 168(a0)
    fsd fs8, 176(a0)
    fsd fs9, 184(a0)
    fsd fs10, 192(a0)
    fsd fs11, 200(a0)
    li a0, 0
    ret

    .global __rl_longjmp
    .hidden __rl_longjmp
    .type __rl_longjmp,%function
__rl_longjmp:
    ld s0, 0(a0)
    ld s1, 8(a0)
    ld s2, 16(a0)
    ld s3, 24(a0)
    ld s4, 32(a0)
    ld s5, 40(a0)
    ld s6, 48(a0)
    ld s7, 56(a0)
    ld s8, 64(a0)
    ld s9, 72(a0)
    ld s10, 80(a0)
    ld s11, 88(a0)
    ld sp, 96(a0)
    ld ra, 104(a0)
    fld fs0, 112(a0)
    fld fs1, 120(a0)
    fld fs2, 128(a0)
    fld fs3, 136(a0)
    fld fs4, 144(a0)
    fld fs5, 152(a0)
    fld fs6, 160(a0)
    fld fs7, 168(a0)
    fld fs8, 176(a0)
    fld fs9, 184(a0)
    fld fs10, 192(a0)
    fld fs11, 200(a0)
    seqz a0, a1
    add a0, a0, a1
    ret

    .global __rl_current_sp
    .hidden __rl_current_sp
    .type __rl_current_sp,%function
__rl_current_sp:
    mv a0, sp
    ret
"#
);

#[cfg(any(raven_va_softfloat, not(target_feature = "d")))]
core::arch::global_asm!(
    r#"
    .text
    .global __rl_setjmp
    .hidden __rl_setjmp
    .type __rl_setjmp,%function
__rl_setjmp:
    sd s0, 0(a0)
    sd s1, 8(a0)
    sd s2, 16(a0)
    sd s3, 24(a0)
    sd s4, 32(a0)
    sd s5, 40(a0)
    sd s6, 48(a0)
    sd s7, 56(a0)
    sd s8, 64(a0)
    sd s9, 72(a0)
    sd s10, 80(a0)
    sd s11, 88(a0)
    sd sp, 96(a0)
    sd ra, 104(a0)
    li a0, 0
    ret

    .global __rl_longjmp
    .hidden __rl_longjmp
    .type __rl_longjmp,%function
__rl_longjmp:
    ld s0, 0(a0)
    ld s1, 8(a0)
    ld s2, 16(a0)
    ld s3, 24(a0)
    ld s4, 32(a0)
    ld s5, 40(a0)
    ld s6, 48(a0)
    ld s7, 56(a0)
    ld s8, 64(a0)
    ld s9, 72(a0)
    ld s10, 80(a0)
    ld s11, 88(a0)
    ld sp, 96(a0)
    ld ra, 104(a0)
    seqz a0, a1
    add a0, a0, a1
    ret

    .global __rl_current_sp
    .hidden __rl_current_sp
    .type __rl_current_sp,%function
__rl_current_sp:
    mv a0, sp
    ret
"#
);
