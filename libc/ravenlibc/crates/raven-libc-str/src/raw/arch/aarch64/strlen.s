// SPDX-License-Identifier: MIT
// Copyright (c) 2012-2023, Arm Limited.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:contrib/arm-optimized-routines/string/aarch64/strlen.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// Scalars: __raven_str_strlen
/*
 * strlen - calculate the length of a string.
 *
 * Copyright (c) 2020-2022, Arm Limited.
 * SPDX-License-Identifier: MIT OR Apache-2.0 WITH LLVM-exception
 */
/* Assumptions:
 *
 * ARMv8-a, AArch64, Advanced SIMD, unaligned accesses.
 * Not MTE compatible.
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
/* For the first 32 bytes, NUL detection works on the principle that
   (X - 1) & (~X) & 0x80 (=> (X - 1) & ~(X | 0x7f)) is non-zero if a
   byte is zero, and can be done in parallel across the entire word.  */
/* To test the page crossing code path more thoroughly, compile with
   -DTEST_PAGE_CROSS - this will force all calls through the slower
   entry path.  This option is not intended for production use.  */
/* Core algorithm:

   Since strings are short on average, we check the first 32 bytes of the
   string for a NUL character without aligning the string.  In order to use
   unaligned loads safely we must do a page cross check first.

   If there is a NUL byte we calculate the length from the 2 8-byte words
   using conditional select to reduce branch mispredictions (it is unlikely
   strlen will be repeatedly called on strings with the same length).

   If the string is longer than 32 bytes, align src so we don't need further
   page cross checks, and process 32 bytes per iteration using a fast SIMD
   loop.

   If the page cross check fails, we read 32 bytes from an aligned address,
   and ignore any characters before the string.  If it contains a NUL
   character, return the length, if not, continue in the main loop.  */
.global __raven_str_strlen; .type __raven_str_strlen,%function; .align 6; __raven_str_strlen: .cfi_startproc; hint 34;
 and x4, x0, 4096 - 1
 cmp x4, 4096 - 32
 b.hi .Lpage_cross
 /* Look for a NUL byte in the first 16 bytes.  */
 ldp x2, x3, [x0]
 mov x8, 0x0101010101010101
 sub x4, x2, x8
 orr x5, x2, 0x7f7f7f7f7f7f7f7f
 sub x6, x3, x8
 orr x7, x3, 0x7f7f7f7f7f7f7f7f
 bics x4, x4, x5
 bic x5, x6, x7
 ccmp x5, 0, 0, eq
 b.eq .Lbytes16_31
 /* Find the exact offset of the first NUL byte in the first 16 bytes
	   from the string start.  Enter with C = has_nul1 == 0.  */
 csel x4, x4, x5, cc
 mov x0, 8
 rev x4, x4
 csel x0, xzr, x0, cc
 clz x4, x4
 add x0, x0, x4, lsr 3
 ret
 /* Look for a NUL byte at offset 16..31 in the string.  */
.Lbytes16_31:
 ldp x2, x3, [x0, 16]
 sub x4, x2, x8
 orr x5, x2, 0x7f7f7f7f7f7f7f7f
 sub x6, x3, x8
 orr x7, x3, 0x7f7f7f7f7f7f7f7f
 bics x4, x4, x5
 bic x5, x6, x7
 ccmp x5, 0, 0, eq
 b.eq .Lloop_entry
 /* Find the exact offset of the first NUL byte at offset 16..31 from
	   the string start.  Enter with C = has_nul1 == 0.  */
 csel x4, x4, x5, cc
 mov x0, 24
 rev x4, x4
 mov x6, 16
 clz x4, x4
 csel x0, x6, x0, cc
 add x0, x0, x4, lsr 3
 ret
 nop
.Lloop_entry:
 bic x1, x0, 31
 .p2align 5
.Lloop:
 ldp q1, q2, [x1, 32]!
 uminp v0.16b, v1.16b, v2.16b
 uminp v0.16b, v0.16b, v0.16b
 cmeq v0.8b, v0.8b, 0
 fmov x3, d0
 cbz x3, .Lloop
 /* Low 32 bits of synd are non-zero if a NUL was found in datav1.  */
 cmeq v0.16b, v1.16b, 0
 sub x0, x1, x0
 cbnz w3, 1f
 cmeq v0.16b, v2.16b, 0
 add x0, x0, 16
1:
 /* Generate a bitmask and compute correct byte offset.  */
 shrn v0.8b, v0.8h, 4
 fmov x3, d0
 rbit x3, x3
 clz x2, x3
 add x0, x0, x2, lsr 2
 ret
.Lpage_cross:
 bic x1, x0, 31
 mov w2, 0x0c03
 movk w2, 0xc030, lsl 16
 ld1 {v1.16b, v2.16b}, [x1]
 dup v0.4s, w2
 cmeq v1.16b, v1.16b, 0
 cmeq v2.16b, v2.16b, 0
 and v1.16b, v1.16b, v0.16b
 and v2.16b, v2.16b, v0.16b
 addp v0.16b, v1.16b, v2.16b
 addp v0.16b, v0.16b, v0.16b
 fmov x3, d0
 lsl x4, x0, 1
 lsr x3, x3, x4
 cbz x3, .Lloop
 rbit x3, x3
 clz x0, x3
 lsr x0, x0, 1
 ret
.cfi_endproc; .size __raven_str_strlen, .-__raven_str_strlen;
