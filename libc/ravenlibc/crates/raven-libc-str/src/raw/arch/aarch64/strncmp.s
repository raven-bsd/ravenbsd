// SPDX-License-Identifier: MIT
// Copyright (c) 2012-2023, Arm Limited.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:contrib/arm-optimized-routines/string/aarch64/strncmp.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// Scalars: __raven_str_strncmp
/*
 * strncmp - compare two strings
 *
 * Copyright (c) 2013-2022, Arm Limited.
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
/* Parameters and result.  */
/* Internal variables.  */
/* Define endian dependent shift operations.
   On big-endian early bytes are at MSB and on little-endian LSB.
   LS_FW means shifting towards early bytes.
   LS_BK means shifting towards later bytes.
   */
.global __raven_str_strncmp; .type __raven_str_strncmp,%function; .align 6; __raven_str_strncmp: .cfi_startproc; hint 34;
 cbz x2, .Lret0
 eor x8, x0, x1
 mov x11, #0x0101010101010101
 tst x8, #7
 and x13, x0, #7
 b.ne .Lmisaligned8
 cbnz x13, .Lmutual_align
 /* NUL detection works on the principle that (X - 1) & (~X) & 0x80
	   (=> (X - 1) & ~(X | 0x7f)) is non-zero iff a byte is zero, and
	   can be done in parallel across the entire word.  */
 .p2align 4
.Lloop_aligned:
 ldr x3, [x0], #8
 ldr x4, [x1], #8
.Lstart_realigned:
 subs x2, x2, #8
 sub x8, x3, x11
 orr x9, x3, #0x7f7f7f7f7f7f7f7f
 eor x6, x3, x4 /* Non-zero if differences found.  */
 csinv x14, x6, xzr, hi /* Last Dword or differences.  */
 bics x5, x8, x9 /* Non-zero if NUL terminator.  */
 ccmp x14, #0, #0, eq
 b.eq .Lloop_aligned
 /* End of main loop */
.Lfull_check:
 orr x7, x6, x5
 add x2, x2, 8 /* Rewind limit to before last subs. */
.Lsyndrome_check:
 /* Limit was reached. Check if the NUL byte or the difference
	   is before the limit. */
 rev x7, x7
 rev x3, x3
 clz x12, x7
 rev x4, x4
 lsl x3, x3, x12
 cmp x2, x12, lsr #3
 lsl x4, x4, x12
 /* But we need to zero-extend (char is unsigned) the value and then
	   perform a signed 32-bit subtraction.  */
 lsr x3, x3, #56
 sub x0, x3, x4, lsr #56
 csel x0, x0, xzr, hi
 ret
.Lmutual_align:
 /* Sources are mutually aligned, but are not currently at an
	   alignment boundary.  Round down the addresses and then mask off
	   the bytes that precede the start point.
	   We also need to adjust the limit calculations, but without
	   overflowing if the limit is near ULONG_MAX.  */
 bic x0, x0, #7
 bic x1, x1, #7
 ldr x3, [x0], #8
 neg x10, x13, lsl #3 /* 64 - bits(bytes beyond align). */
 ldr x4, [x1], #8
 mov x9, #~0
 lsr x9, x9, x10 /* Shift (count & 63).  */
 /* Adjust the limit and ensure it doesn't overflow.  */
 adds x2, x2, x13
 csinv x2, x2, xzr, lo
 orr x3, x3, x9
 orr x4, x4, x9
 b .Lstart_realigned
 .p2align 4
 /* Don't bother with dwords for up to 16 bytes.  */
.Lmisaligned8:
 cmp x2, #16
 b.hs .Ltry_misaligned_words
.Lbyte_loop:
 /* Perhaps we can do better than this.  */
 ldrb w3, [x0], #1
 ldrb w4, [x1], #1
 subs x2, x2, #1
 ccmp w3, #1, #0, hi /* NZCV = 0b0000.  */
 ccmp w3, w4, #0, cs /* NZCV = 0b0000.  */
 b.eq .Lbyte_loop
.Ldone:
 sub x0, x3, x4
 ret
 /* Align the SRC1 to a dword by doing a bytewise compare and then do
	   the dword loop.  */
.Ltry_misaligned_words:
 cbz x13, .Lsrc1_aligned
 neg x13, x13
 and x13, x13, #7
 sub x2, x2, x13
.Lpage_end_loop:
 ldrb w3, [x0], #1
 ldrb w4, [x1], #1
 cmp w3, #1
 ccmp w3, w4, #0, cs /* NZCV = 0b0000.  */
 b.ne .Ldone
 subs x13, x13, #1
 b.hi .Lpage_end_loop
 /* The following diagram explains the comparison of misaligned strings.
	   The bytes are shown in natural order. For little-endian, it is
	   reversed in the registers. The "x" bytes are before the string.
	   The "|" separates data that is loaded at one time.
	   src1     | a a a a a a a a | b b b c c c c c | . . .
	   src2     | x x x x x a a a   a a a a a b b b | c c c c c . . .

	   After shifting in each step, the data looks like this:
	                STEP_A              STEP_B              STEP_C
	   data1    a a a a a a a a     b b b c c c c c     b b b c c c c c
	   data2    a a a a a a a a     b b b 0 0 0 0 0     0 0 0 c c c c c

	   The bytes with "0" are eliminated from the syndrome via mask.

	   Align SRC2 down to 16 bytes. This way we can read 16 bytes at a
	   time from SRC2. The comparison happens in 3 steps. After each step
	   the loop can exit, or read from SRC1 or SRC2. */
.Lsrc1_aligned:
 /* Calculate offset from 8 byte alignment to string start in bits. No
	   need to mask offset since shifts are ignoring upper bits. */
 lsl x12, x1, #3
 bic x1, x1, #0xf
 mov x13, -1
 neg x15, x12
 ldr x3, [x0], #8
 ldp x8, x9, [x1], #16
 lsl x13, x13, x15
 and x15, x15, #63 /* Need actual value for cmp later. */
 /* Skip the first compare if data in tmp1 is irrelevant. */
 tbnz x12, 6, .Lmisaligned_mid_loop
.Lloop_misaligned:
 /* STEP_A: Compare full 8 bytes when there is enough data from SRC2.*/
 lsr x4, x8, x12
 lsl x8, x9, x15
 subs x2, x2, #8
 orr x4, x4, x8 /* 8 bytes from SRC2 combined from two regs.*/
 sub x5, x3, x11
 eor x6, x3, x4 /* Non-zero if differences found.  */
 orr x10, x3, #0x7f7f7f7f7f7f7f7f
 csinv x14, x6, xzr, hi /* If limit, set to all ones. */
 bic x5, x5, x10 /* Non-zero if NUL byte found in SRC1. */
 orr x10, x14, x5
 cbnz x10, .Lfull_check
 ldr x3, [x0], #8
.Lmisaligned_mid_loop:
 /* STEP_B: Compare first part of data1 to second part of tmp2. */
 lsr x4, x9, x12
 sub x5, x3, x11
 orr x10, x3, #0x7f7f7f7f7f7f7f7f
 eor x6, x4, x3 /* Non-zero if differences found.  */
 bic x5, x5, x10 /* Non-zero if NUL terminator.  */
 cmp x2, x15, lsr #3
 orr x7, x6, x5
 bic x7, x7, x13 /* Ignore later bytes. */
 csinv x10, x7, xzr, hi /* If limit, set to all ones. */
 cbnz x10, .Lsyndrome_check
 /* STEP_C: Compare second part of data1 to first part of tmp1. */
 ldp x8, x9, [x1], #16
 cmp x2, #8
 lsl x4, x8, x15
 eor x6, x4, x3 /* Non-zero if differences found.  */
 orr x7, x6, x5
 and x7, x7, x13 /* Ignore earlier bytes. */
 csinv x10, x7, xzr, hi /* If limit, set to all ones. */
 cbnz x10, .Lsyndrome_check
 ldr x3, [x0], #8
 sub x2, x2, #8
 b .Lloop_misaligned
.Lret0:
 mov x0, #0
 ret
.cfi_endproc; .size __raven_str_strncmp, .-__raven_str_strncmp;
