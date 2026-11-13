// SPDX-License-Identifier: MIT
// Copyright (c) 2012-2023, Arm Limited.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:contrib/arm-optimized-routines/string/aarch64/memcmp.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (the donor is MIT OR Apache-2.0 WITH LLVM-exception; MIT elected. Preprocessed once:
//  ENTRY/END/L/BTI_C/IFSTPCPY expanded, .note.gnu.property dropped, symbols renamed)

//
// Permission is hereby granted, free of charge, to any person obtaining a copy
// of this software and associated documentation files (the "Software"), to deal
// in the Software without restriction, including without limitation the rights
// to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
// copies of the Software, and to permit persons to whom the Software is
// furnished to do so, subject to the following conditions:
// 
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
// 
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.
//
// Scalars: __raven_str_memcmp
/* memcmp - compare memory
 *
 * Copyright (c) 2013-2022, Arm Limited.
 * SPDX-License-Identifier: MIT OR Apache-2.0 WITH LLVM-exception
 */
/* Assumptions:
 *
 * ARMv8-a, AArch64, Advanced SIMD, unaligned accesses.
 */
/*
 * Macros for asm code.  AArch64 version.
 *
 * Copyright (c) 2019-2023, Arm Limited.
 * SPDX-License-Identifier: MIT OR Apache-2.0 WITH LLVM-exception
 */
/* Branch Target Identitication support.  */
/* Return address signing support (pac-ret).  */
/* GNU_PROPERTY_AARCH64_* macros from elf.h.  */
/* Add a NT_GNU_PROPERTY_TYPE_0 note.  */
/* If set then the GNU Property Note section will be added to
   mark objects to support BTI and PAC-RET.  */
.global __raven_str_memcmp; .type __raven_str_memcmp,%function; .align 6; __raven_str_memcmp: .cfi_startproc; hint 34;
 cmp x2, 16
 b.lo .Lless16
 ldp x3, x5, [x0]
 ldp x4, x6, [x1]
 ccmp x3, x4, 0, ne
 ccmp x5, x6, 0, eq
 b.ne .Lreturn2
 add x7, x0, x2
 add x8, x1, x2
 cmp x2, 32
 b.ls .Llast_bytes
 cmp x2, 160
 b.hs .Lloop_align
 sub x2, x2, 32
 .p2align 4
.Lloop32:
 ldp x3, x5, [x0, 16]
 ldp x4, x6, [x1, 16]
 cmp x3, x4
 ccmp x5, x6, 0, eq
 b.ne .Lreturn2
 cmp x2, 16
 b.ls .Llast_bytes
 ldp x3, x5, [x0, 32]
 ldp x4, x6, [x1, 32]
 cmp x3, x4
 ccmp x5, x6, 0, eq
 b.ne .Lreturn2
 add x0, x0, 32
 add x1, x1, 32
.Llast64:
 subs x2, x2, 32
 b.hi .Lloop32
 /* Compare last 1-16 bytes using unaligned access.  */
.Llast_bytes:
 ldp x3, x5, [x7, -16]
 ldp x4, x6, [x8, -16]
.Lreturn2:
 cmp x3, x4
 csel x3, x3, x5, ne
 csel x4, x4, x6, ne
 /* Compare data bytes and set return value to 0, -1 or 1.  */
.Lreturn:
 rev x3, x3
 rev x4, x4
 cmp x3, x4
 cset w0, ne
 cneg w0, w0, lo
 ret
 .p2align 4
.Lless16:
 add x7, x0, x2
 add x8, x1, x2
 tbz x2, 3, .Lless8
 ldr x3, [x0]
 ldr x4, [x1]
 ldr x5, [x7, -8]
 ldr x6, [x8, -8]
 b .Lreturn2
 .p2align 4
.Lless8:
 tbz x2, 2, .Lless4
 ldr w3, [x0]
 ldr w4, [x1]
 ldr w5, [x7, -4]
 ldr w6, [x8, -4]
 b .Lreturn2
.Lless4:
 tbz x2, 1, .Lless2
 ldrh w3, [x0]
 ldrh w4, [x1]
 cmp w3, w4
 b.ne .Lreturn
.Lless2:
 mov w0, 0
 tbz x2, 0, .Lreturn_zero
 ldrb w3, [x7, -1]
 ldrb w4, [x8, -1]
 sub w0, w3, w4
.Lreturn_zero:
 ret
.Lloop_align:
 ldp x3, x5, [x0, 16]
 ldp x4, x6, [x1, 16]
 cmp x3, x4
 ccmp x5, x6, 0, eq
 b.ne .Lreturn2
 /* Align src2 and adjust src1, src2 and limit.  */
 and x6, x1, 15
 sub x6, x6, 16
 sub x1, x1, x6
 add x2, x2, x6
 sub x0, x0, x6
 sub x2, x2, 64 + 16
 .p2align 4
.Lloop64:
 ldr q0, [x0, 16]
 ldr q1, [x1, 16]
 subs x2, x2, 64
 ldr q2, [x0, 32]
 ldr q3, [x1, 32]
 eor v0.16b, v0.16b, v1.16b
 eor v1.16b, v2.16b, v3.16b
 ldr q2, [x0, 48]
 ldr q3, [x1, 48]
 umaxp v0.16b, v0.16b, v1.16b
 ldr q4, [x0, 64]!
 ldr q5, [x1, 64]!
 eor v1.16b, v2.16b, v3.16b
 eor v2.16b, v4.16b, v5.16b
 umaxp v1.16b, v1.16b, v2.16b
 umaxp v0.16b, v0.16b, v1.16b
 umaxp v0.16b, v0.16b, v0.16b
 fmov x6, d0
 ccmp x6, 0, 0, hi
 b.eq .Lloop64
 /* If equal, process last 1-64 bytes using scalar loop.  */
 add x2, x2, 64 + 16
 cbz x6, .Llast64
 /* Determine the 8-byte aligned offset of the first difference.  */
 rev x6, x6
 clz x6, x6
 bic x6, x6, 7
 sub x6, x6, 48
 ldr x3, [x0, x6]
 ldr x4, [x1, x6]
 rev x3, x3
 rev x4, x4
 mov w0, 1
 cmp x3, x4
 cneg w0, w0, lo
 ret
.cfi_endproc; .size __raven_str_memcmp, .-__raven_str_memcmp;
