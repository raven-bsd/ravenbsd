// SPDX-License-Identifier: MIT
// Copyright (c) 2012-2023, Arm Limited.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:contrib/arm-optimized-routines/string/aarch64/strrchr.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// Scalars: __raven_str_strrchr
/*
 * strrchr - find last position of a character in a string.
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
   the relevant byte matched the requested character; bit 1 is set
   iff the relevant byte matched the NUL end of string (we trigger
   off bit0 for the special case of looking for NUL).  Since the bits
   in the syndrome reflect exactly the order in which things occur
   in the original string a count_trailing_zeros() operation will
   identify exactly which byte is causing the termination, and why.  */
.global __raven_str_strrchr; .type __raven_str_strrchr,%function; .align 6; __raven_str_strrchr: .cfi_startproc; hint 34;
 /* Magic constant 0x40100401 to allow us to identify which lane
	   matches the requested byte.  Magic constant 0x80200802 used
	   similarly for NUL termination.  */
 mov w4, #0x0401
 movk w4, #0x4010, lsl #16
 dup v0.16b, w1
 bic x2, x0, #31 /* Work with aligned 32-byte hunks.  */
 dup v16.4s, w4
 mov x7, #0
 ands x3, x0, #31
 add v7.4s, v16.4s, v16.4s /* equiv: lsl #1 */
 b.eq .Laligned
 /* Input string is not 32-byte aligned.  Rather than forcing
	   the padding bytes to a safe value, we calculate the syndrome
	   for all the bytes, but then mask off those bits of the
	   syndrome that are related to the padding.  */
 ld1 {v1.16b, v2.16b}, [x2], #32
 neg x3, x3
 cmeq v3.16b, v1.16b, #0
 cmeq v5.16b, v1.16b, v0.16b
 cmeq v4.16b, v2.16b, #0
 cmeq v6.16b, v2.16b, v0.16b
 and v3.16b, v3.16b, v7.16b
 and v5.16b, v5.16b, v16.16b
 and v4.16b, v4.16b, v7.16b
 and v6.16b, v6.16b, v16.16b
 addp v3.16b, v3.16b, v4.16b // 256->128
 addp v5.16b, v5.16b, v6.16b // 256->128
 addp v17.16b, v3.16b, v5.16b // 128->64
 mov x10, v17.d[0]
 lsl x3, x3, #1
 mov x8, #~0
 lsr x5, x8, x3
 mov x11, v17.d[1]
 bic x10, x10, x5 // Mask padding bits.
 bic x11, x11, x5 // Mask padding bits.
 cbnz x10, .Ltail
 .p2align 4
.Lloop:
 cmp x11, #0
 csel x6, x2, x6, ne
 csel x7, x11, x7, ne
.Laligned:
 ld1 {v1.16b, v2.16b}, [x2], #32
 cmeq v5.16b, v1.16b, v0.16b
 cmeq v6.16b, v2.16b, v0.16b
 uminp v17.16b, v1.16b, v2.16b
 and v5.16b, v5.16b, v16.16b
 and v6.16b, v6.16b, v16.16b
 cmeq v17.16b, v17.16b, 0
 addp v5.16b, v5.16b, v6.16b // 256->128
 addp v17.16b, v17.16b, v5.16b // 128->64
 mov x10, v17.d[0]
 mov x11, v17.d[1]
 cbz x10, .Lloop
 cmeq v3.16b, v1.16b, #0
 cmeq v4.16b, v2.16b, #0
 and v3.16b, v3.16b, v7.16b
 and v4.16b, v4.16b, v7.16b
 addp v3.16b, v3.16b, v4.16b
 addp v3.16b, v3.16b, v3.16b
 mov x10, v3.d[0]
.Ltail:
 /* Work out exactly where the string ends.  */
 sub x9, x10, #1
 eor x9, x9, x10
 ands x11, x11, x9
 /* And pick the values corresponding to the last match.  */
 csel x6, x2, x6, ne
 csel x7, x11, x7, ne
 /* Count down from the top of the syndrome to find the last match.  */
 clz x5, x7
 /* Src_match points beyond the word containing the match, so we can
	   simply subtract half the bit-offset into the syndrome.  Because
	   we are counting down, we need to go back one more character.  */
 add x5, x5, #2
 sub x0, x6, x5, lsr #1
 /* But if the syndrome shows no match was found, then return NULL.  */
 cmp x7, #0
 csel x0, x0, xzr, ne
 ret
.cfi_endproc; .size __raven_str_strrchr, .-__raven_str_strrchr;
