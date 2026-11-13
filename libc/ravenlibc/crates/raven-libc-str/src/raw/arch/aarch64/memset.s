// SPDX-License-Identifier: MIT
// Copyright (c) 2012-2023, Arm Limited.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:contrib/arm-optimized-routines/string/aarch64/memset.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// Scalars: __raven_str_memset
/*
 * memset - fill memory with a constant byte
 *
 * Copyright (c) 2012-2024, Arm Limited.
 * SPDX-License-Identifier: MIT OR Apache-2.0 WITH LLVM-exception
 */
/* Assumptions:
 *
 * ARMv8-a, AArch64, Advanced SIMD, unaligned accesses.
 *
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
.global __raven_str_memset; .type __raven_str_memset,%function; .align 6; __raven_str_memset: .cfi_startproc; hint 34;
 dup v0.16B, w1
 cmp x2, 16
 b.lo .Lset_small
 add x4, x0, x2
 cmp x2, 64
 b.hs .Lset_128
 /* Set 16..63 bytes.  */
 mov x3, 16
 and x3, x3, x2, lsr 1
 sub x5, x4, x3
 str q0, [x0]
 str q0, [x0, x3]
 str q0, [x5, -16]
 str q0, [x4, -16]
 ret
 .p2align 4
 /* Set 0..15 bytes.  */
.Lset_small:
 add x4, x0, x2
 cmp x2, 4
 b.lo 2f
 lsr x3, x2, 3
 sub x5, x4, x3, lsl 2
 str s0, [x0]
 str s0, [x0, x3, lsl 2]
 str s0, [x5, -4]
 str s0, [x4, -4]
 ret
 /* Set 0..3 bytes.  */
2: cbz x2, 3f
 lsr x3, x2, 1
 strb w1, [x0]
 strb w1, [x0, x3]
 strb w1, [x4, -1]
3: ret
 .p2align 4
.Lset_128:
 bic x3, x0, 15
 cmp x2, 128
 b.hi .Lset_long
 stp q0, q0, [x0]
 stp q0, q0, [x0, 32]
 stp q0, q0, [x4, -64]
 stp q0, q0, [x4, -32]
 ret
 .p2align 4
.Lset_long:
 str q0, [x0]
 str q0, [x3, 16]
 tst w1, 255
 b.ne .Lno_zva
 mrs x5, dczid_el0
 and x5, x5, 31
 cmp x5, 4 /* ZVA size is 64 bytes.  */
 b.ne .Lno_zva
 stp q0, q0, [x3, 32]
 bic x3, x0, 63
 sub x2, x4, x3 /* Count is now 64 too large.  */
 sub x2, x2, 64 + 64 /* Adjust count and bias for loop.  */
 /* Write last bytes before ZVA loop.  */
 stp q0, q0, [x4, -64]
 stp q0, q0, [x4, -32]
 .p2align 4
.Lzva64_loop:
 add x3, x3, 64
 dc zva, x3
 subs x2, x2, 64
 b.hi .Lzva64_loop
 ret
 .p2align 3
.Lno_zva:
 sub x2, x4, x3 /* Count is 32 too large.  */
 sub x2, x2, 64 + 32 /* Adjust count and bias for loop.  */
.Lno_zva_loop:
 stp q0, q0, [x3, 32]
 stp q0, q0, [x3, 64]
 add x3, x3, 64
 subs x2, x2, 64
 b.hi .Lno_zva_loop
 stp q0, q0, [x4, -64]
 stp q0, q0, [x4, -32]
 ret
.cfi_endproc; .size __raven_str_memset, .-__raven_str_memset;
