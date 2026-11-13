// SPDX-License-Identifier: MIT
// Copyright (c) 2012-2023, Arm Limited.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:contrib/arm-optimized-routines/string/aarch64/strcmp.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// Scalars: __raven_str_strcmp
/*
 * strcmp - compare two strings
 *
 * Copyright (c) 2012-2022, Arm Limited.
 * SPDX-License-Identifier: MIT OR Apache-2.0 WITH LLVM-exception
 */
/* Assumptions:
 *
 * ARMv8-a, AArch64.
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
/* On big-endian early bytes are at MSB and on little-endian LSB.
   LS_FW means shifting towards early bytes.  */
/* NUL detection works on the principle that (X - 1) & (~X) & 0x80
   (=> (X - 1) & ~(X | 0x7f)) is non-zero iff a byte is zero, and
   can be done in parallel across the entire word.
   Since carry propagation makes 0x1 bytes before a NUL byte appear
   NUL too in big-endian, byte-reverse the data before the NUL check.  */
.global __raven_str_strcmp; .type __raven_str_strcmp,%function; .align 6; __raven_str_strcmp: .cfi_startproc; hint 34;
 sub x10, x1, x0
 mov x8, 0x0101010101010101
 and x6, x0, 7
 tst x10, 7
 b.ne .Lmisaligned8
 cbnz x6, .Lmutual_align
 .p2align 4
.Lloop_aligned:
 ldr x3, [x0, x10]
 ldr x2, [x0], 8
.Lstart_realigned:
 sub x4, x2, x8
 orr x6, x2, 0x7f7f7f7f7f7f7f7f
 bics x4, x4, x6 /* Non-zero if NUL terminator.  */
 ccmp x2, x3, 0, eq
 b.eq .Lloop_aligned
 eor x5, x2, x3
 orr x6, x5, x4
.Lend:
 rev x6, x6
 rev x2, x2
 rev x3, x3
 clz x9, x6
 /* The most-significant-non-zero bit of the syndrome marks either the
	   first bit that is different, or the top bit of the first zero byte.
	   Shifting left now will bring the critical information into the
	   top bits.  */
 lsl x2, x2, x9
 lsl x3, x3, x9
 /* But we need to zero-extend (char is unsigned) the value and then
	   perform a signed 32-bit subtraction.  */
 lsr x2, x2, 56
 sub x0, x2, x3, lsr 56
 ret
 .p2align 4
.Lmutual_align:
 /* Sources are mutually aligned, but are not currently at an
	   alignment boundary.  Round down the addresses and then mask off
	   the bytes that precede the start point.  */
 bic x0, x0, 7
 ldr x3, [x0, x10]
 ldr x2, [x0], 8
 neg x9, x1, lsl 3 /* Bits to alignment -64.  */
 mov x6, -1
 lsr x6, x6, x9
 orr x2, x2, x6
 orr x3, x3, x6
 b .Lstart_realigned
.Lmisaligned8:
 /* Align SRC1 to 8 bytes and then compare 8 bytes at a time, always
	   checking to make sure that we don't access beyond the end of SRC2.  */
 cbz x6, .Lsrc1_aligned
.Ldo_misaligned:
 ldrb w2, [x0], 1
 ldrb w3, [x1], 1
 cmp w2, 0
 ccmp w2, w3, 0, ne /* NZCV = 0b0000.  */
 b.ne .Ldone
 tst x0, 7
 b.ne .Ldo_misaligned
.Lsrc1_aligned:
 neg x9, x1, lsl 3
 bic x1, x1, 7
 ldr x7, [x1], 8
 lsr x6, x8, x9
 orr x7, x7, x6
 sub x4, x7, x8
 orr x6, x7, 0x7f7f7f7f7f7f7f7f
 bics x4, x4, x6
 b.ne .Ltail
 sub x5, x1, x0
 .p2align 4
.Lloop_unaligned:
 ldr x7, [x0, x5]
 ldr x3, [x0, x10]
 sub x4, x7, x8
 orr x6, x7, 0x7f7f7f7f7f7f7f7f
 ldr x2, [x0], 8
 bics x4, x4, x6
 ccmp x2, x3, 0, eq
 b.eq .Lloop_unaligned
 lsl x6, x4, x9
 eor x5, x2, x3
 orr x6, x5, x6
 cbnz x6, .Lend
.Ltail:
 ldr x2, [x0]
 neg x9, x9
 lsr x3, x7, x9
 lsr x4, x4, x9
 eor x5, x2, x3
 orr x6, x5, x4
 b .Lend
.Ldone:
 sub x0, x2, x3
 ret
.cfi_endproc; .size __raven_str_strcmp, .-__raven_str_strcmp;
