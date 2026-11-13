// SPDX-License-Identifier: MIT
// Copyright (c) 2012-2023, Arm Limited.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:contrib/arm-optimized-routines/string/aarch64/strchrnul.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// Scalars: __raven_str_strchr
// Scalars: __raven_str_strchrnul
/*
 * strchrnul - find a character or nul in a string
 *
 * Copyright (c) 2014-2022, Arm Limited.
 * SPDX-License-Identifier: MIT OR Apache-2.0 WITH LLVM-exception
 */
/* Assumptions:
 *
 * ARMv8-a, AArch64
 * Neon Available.
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

/* Arguments and results.  */
/* Core algorithm.

   For each 32-byte hunk we calculate a 64-bit syndrome value, with
   two bits per byte (LSB is always in bits 0 and 1, for both big
   and little-endian systems).  For each tuple, bit 0 is set iff
   the relevant byte matched the requested character or nul.  Since the
   bits in the syndrome reflect exactly the order in which things occur
   in the original string a count_trailing_zeros() operation will
   identify exactly which byte is causing the termination.  */
/* Locals and temporaries.  */
.global __raven_str_strchrnul; .type __raven_str_strchrnul,%function; .align 6; __raven_str_strchrnul: .cfi_startproc; hint 34;
 /* Magic constant 0x40100401 to allow us to identify which lane
	   matches the termination condition.  */
 mov w4, #0x0401
 movk w4, #0x4010, lsl #16
 dup v0.16b, w1
 bic x2, x0, #31 /* Work with aligned 32-byte hunks.  */
 dup v7.4s, w4
 ands x3, x0, #31
 b.eq .Lloop
 /* Input string is not 32-byte aligned.  Rather than forcing
	   the padding bytes to a safe value, we calculate the syndrome
	   for all the bytes, but then mask off those bits of the
	   syndrome that are related to the padding.  */
 ld1 {v1.16b, v2.16b}, [x2], #32
 neg x3, x3
 cmeq v5.16b, v1.16b, v0.16b
 cmeq v6.16b, v2.16b, v0.16b
 cmhs v3.16b, v5.16b, v1.16b
 cmhs v4.16b, v6.16b, v2.16b
 and v5.16b, v3.16b, v7.16b
 and v6.16b, v4.16b, v7.16b
 lsl x3, x3, #1
 addp v16.16b, v5.16b, v6.16b // 256->128
 mov x5, #~0
 addp v16.16b, v16.16b, v16.16b // 128->64
 lsr x3, x5, x3
 mov x5, v16.d[0]
 bic x3, x5, x3 // Mask padding bits.
 cbnz x3, .Ltail
 .p2align 4
.Lloop:
 ld1 {v1.16b, v2.16b}, [x2], #32
 cmeq v5.16b, v1.16b, v0.16b
 cmeq v6.16b, v2.16b, v0.16b
 cmhs v3.16b, v5.16b, v1.16b
 cmhs v4.16b, v6.16b, v2.16b
 orr v16.16b, v3.16b, v4.16b
 umaxp v16.16b, v16.16b, v16.16b
 mov x3, v16.d[0]
 cbz x3, .Lloop
 /* Termination condition found.  Now need to establish exactly why
	   we terminated.  */
 and v5.16b, v3.16b, v7.16b
 and v6.16b, v4.16b, v7.16b
 addp v16.16b, v5.16b, v6.16b // 256->128
 addp v16.16b, v16.16b, v16.16b // 128->64
 mov x3, v16.d[0]
.Ltail:
 /* Count the trailing zeros, by bit reversing...  */
 rbit x3, x3
 /* Re-bias source.  */
 sub x2, x2, #32
 clz x3, x3 /* ... and counting the leading zeros.  */
 /* tmp1 is twice the offset into the fragment.  */
 add x0, x2, x3, lsr #1
 ret
.cfi_endproc; .size __raven_str_strchrnul, .-__raven_str_strchrnul;
