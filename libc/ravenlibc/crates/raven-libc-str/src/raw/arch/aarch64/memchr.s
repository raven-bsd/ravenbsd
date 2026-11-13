// SPDX-License-Identifier: MIT
// Copyright (c) 2012-2023, Arm Limited.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:contrib/arm-optimized-routines/string/aarch64/memchr.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// Scalars: __raven_str_memchr
/*
 * memchr - find a character in a memory zone
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
/*
 * Core algorithm:
 *
 * For each 32-byte chunk we calculate a 64-bit syndrome value, with two bits
 * per byte. For each tuple, bit 0 is set if the relevant byte matched the
 * requested character and bit 1 is not used (faster than using a 32bit
 * syndrome). Since the bits in the syndrome reflect exactly the order in which
 * things occur in the original string, counting trailing zeros allows to
 * identify exactly which byte has matched.
 */
.global __raven_str_memchr; .type __raven_str_memchr,%function; .align 6; __raven_str_memchr: .cfi_startproc; hint 34;
 /* Do not dereference srcin if no bytes to compare.  */
 cbz x2, .Lzero_length
 /*
	 * Magic constant 0x40100401 allows us to identify which lane matches
	 * the requested byte.
	 */
 mov w5, #0x0401
 movk w5, #0x4010, lsl #16
 dup v0.16b, w1
 /* Work with aligned 32-byte chunks */
 bic x3, x0, #31
 dup v5.4s, w5
 ands x9, x0, #31
 and x10, x2, #31
 b.eq .Lloop
 /*
	 * Input string is not 32-byte aligned. We calculate the syndrome
	 * value for the aligned 32 bytes block containing the first bytes
	 * and mask the irrelevant part.
	 */
 ld1 {v1.16b, v2.16b}, [x3], #32
 sub x4, x9, #32
 adds x2, x2, x4
 cmeq v3.16b, v1.16b, v0.16b
 cmeq v4.16b, v2.16b, v0.16b
 and v3.16b, v3.16b, v5.16b
 and v4.16b, v4.16b, v5.16b
 addp v6.16b, v3.16b, v4.16b /* 256->128 */
 addp v6.16b, v6.16b, v6.16b /* 128->64 */
 mov x6, v6.d[0]
 /* Clear the soff*2 lower bits */
 lsl x4, x9, #1
 lsr x6, x6, x4
 lsl x6, x6, x4
 /* The first block can also be the last */
 b.ls .Lmasklast
 /* Have we found something already? */
 cbnz x6, .Ltail
.Lloop:
 ld1 {v1.16b, v2.16b}, [x3], #32
 subs x2, x2, #32
 cmeq v3.16b, v1.16b, v0.16b
 cmeq v4.16b, v2.16b, v0.16b
 /* If we're out of data we finish regardless of the result */
 b.ls .Lend
 /* Use a fast check for the termination condition */
 orr v6.16b, v3.16b, v4.16b
 addp v6.2d, v6.2d, v6.2d
 mov x6, v6.d[0]
 /* We're not out of data, loop if we haven't found the character */
 cbz x6, .Lloop
.Lend:
 /* Termination condition found, let's calculate the syndrome value */
 and v3.16b, v3.16b, v5.16b
 and v4.16b, v4.16b, v5.16b
 addp v6.16b, v3.16b, v4.16b /* 256->128 */
 addp v6.16b, v6.16b, v6.16b /* 128->64 */
 mov x6, v6.d[0]
 /* Only do the clear for the last possible block */
 b.hs .Ltail
.Lmasklast:
 /* Clear the (32 - ((cntrem + soff) % 32)) * 2 upper bits */
 add x4, x10, x9
 and x4, x4, #31
 sub x4, x4, #32
 neg x4, x4, lsl #1
 lsl x6, x6, x4
 lsr x6, x6, x4
.Ltail:
 /* Count the trailing zeros using bit reversing */
 rbit x6, x6
 /* Compensate the last post-increment */
 sub x3, x3, #32
 /* Check that we have found a character */
 cmp x6, #0
 /* And count the leading zeros */
 clz x6, x6
 /* Compute the potential result */
 add x0, x3, x6, lsr #1
 /* Select result or NULL */
 csel x0, xzr, x0, eq
 ret
.Lzero_length:
 mov x0, #0
 ret
.cfi_endproc; .size __raven_str_memchr, .-__raven_str_memchr;
