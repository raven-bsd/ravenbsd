// SPDX-License-Identifier: MIT
// Copyright (c) 2012-2023, Arm Limited.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:contrib/arm-optimized-routines/string/aarch64/memrchr.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// Scalars: __raven_str_memrchr
/*
 * memrchr - find last character in a memory zone.
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
   For each 16-byte chunk we calculate a 64-bit nibble mask value with four bits
   per byte. We take 4 bits of every comparison byte with shift right and narrow
   by 4 instruction. Since the bits in the nibble mask reflect the order in
   which things occur in the original string, counting leading zeros identifies
   exactly which byte matched.  */
.global __raven_str_memrchr; .type __raven_str_memrchr,%function; .align 6; __raven_str_memrchr: .cfi_startproc; hint 34;
 add x8, x0, x2
 sub x9, x8, 1
 bic x3, x9, 15
 cbz x2, .Lnomatch
 ld1 {v1.16b}, [x3]
 dup v0.16b, w1
 cmeq v2.16b, v1.16b, v0.16b
 neg x6, x8, lsl 2
 shrn v3.8b, v2.8h, 4 /* 128->64 */
 fmov x5, d3
 lsl x5, x5, x6
 cbz x5, .Lstart_loop
 clz x5, x5
 sub x0, x9, x5, lsr 2
 cmp x2, x5, lsr 2
 csel x0, x0, xzr, hi
 ret
 nop
.Lstart_loop:
 subs x4, x3, x0
 b.ls .Lnomatch
 /* Make sure that it won't overread by a 16-byte chunk */
 sub x4, x4, 1
 tbz x4, 4, .Lloop32_2
 add x3, x3, 16
 .p2align 5
.Lloop32:
 ldr q1, [x3, -32]!
 cmeq v2.16b, v1.16b, v0.16b
 umaxp v3.16b, v2.16b, v2.16b /* 128->64 */
 fmov x5, d3
 cbnz x5, .Lend
.Lloop32_2:
 ldr q1, [x3, -16]
 subs x4, x4, 32
 cmeq v2.16b, v1.16b, v0.16b
 b.lo .Lend_2
 umaxp v3.16b, v2.16b, v2.16b /* 128->64 */
 fmov x5, d3
 cbz x5, .Lloop32
.Lend_2:
 sub x3, x3, 16
.Lend:
 shrn v3.8b, v2.8h, 4 /* 128->64 */
 fmov x5, d3
 add x7, x3, 15
 clz x5, x5
 sub x7, x7, x5, lsr 2
 cmp x7, x0
 csel x0, x7, xzr, hs
 ret
.Lnomatch:
 mov x0, 0
 ret
.cfi_endproc; .size __raven_str_memrchr, .-__raven_str_memrchr;
