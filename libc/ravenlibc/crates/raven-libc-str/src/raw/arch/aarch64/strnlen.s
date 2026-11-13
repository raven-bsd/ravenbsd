// SPDX-License-Identifier: MIT
// Copyright (c) 2012-2023, Arm Limited.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:contrib/arm-optimized-routines/string/aarch64/strnlen.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// Scalars: __raven_str_strnlen
/*
 * strnlen - calculate the length of a string with limit.
 *
 * Copyright (c) 2020-2022, Arm Limited.
 * SPDX-License-Identifier: MIT OR Apache-2.0 WITH LLVM-exception
 */
/* Assumptions:
 *
 * ARMv8-a, AArch64, Advanced SIMD.
 * MTE compatible.
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
/*
   Core algorithm:
   Process the string in 16-byte aligned chunks. Compute a 64-bit mask with
   four bits per byte using the shrn instruction. A count trailing zeros then
   identifies the first zero byte.  */
.global __raven_str_strnlen; .type __raven_str_strnlen,%function; .align 6; __raven_str_strnlen: .cfi_startproc; hint 34;
 bic x2, x0, 15
 cbz x1, .Lnomatch
 ld1 {v0.16b}, [x2]
 cmeq v1.16b, v0.16b, 0
 lsl x4, x0, 2
 shrn v2.8b, v1.8h, 4 /* 128->64 */
 fmov x3, d2
 lsr x3, x3, x4
 cbz x3, .Lstart_loop
.Lfinish:
 rbit x3, x3
 clz x3, x3
 lsr x0, x3, 2
 cmp x1, x0
 csel x0, x1, x0, ls
 ret
.Lnomatch:
 mov x0, x1
 ret
.Lstart_loop:
 sub x4, x2, x0
 add x4, x4, 17
 subs x5, x1, x4
 b.lo .Lnomatch
 /* Make sure that it won't overread by a 16-byte chunk */
 tbz x5, 4, .Lloop32_2
 sub x2, x2, 16
 .p2align 5
.Lloop32:
 ldr q0, [x2, 32]!
 cmeq v1.16b, v0.16b, 0
 umaxp v2.16b, v1.16b, v1.16b /* 128->64 */
 fmov x3, d2
 cbnz x3, .Lend
.Lloop32_2:
 ldr q0, [x2, 16]
 subs x5, x5, 32
 cmeq v1.16b, v0.16b, 0
 b.lo .Lend_2
 umaxp v2.16b, v1.16b, v1.16b /* 128->64 */
 fmov x3, d2
 cbz x3, .Lloop32
.Lend_2:
 add x2, x2, 16
.Lend:
 shrn v2.8b, v1.8h, 4 /* 128->64 */
 sub x0, x2, x0
 fmov x3, d2
 rbit x3, x3
 clz x3, x3
 add x0, x0, x3, lsr 2
 cmp x1, x0
 csel x0, x1, x0, ls
 ret
.cfi_endproc; .size __raven_str_strnlen, .-__raven_str_strnlen;
