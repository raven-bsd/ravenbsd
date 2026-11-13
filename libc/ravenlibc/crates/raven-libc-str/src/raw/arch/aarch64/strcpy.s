// SPDX-License-Identifier: MIT
// Copyright (c) 2012-2023, Arm Limited.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:contrib/arm-optimized-routines/string/aarch64/strcpy.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
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
// Scalars: __raven_str_strcpy
/*
 * strcpy/stpcpy - copy a string returning pointer to start/end.
 *
 * Copyright (c) 2020-2023, Arm Limited.
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
.global __raven_str_strcpy; .type __raven_str_strcpy,%function; .align 6; __raven_str_strcpy: .cfi_startproc; hint 34;
 bic x2, x1, 15
 ld1 {v0.16b}, [x2]
 cmeq v1.16b, v0.16b, 0
 lsl x5, x1, 2
 shrn v2.8b, v1.8h, 4
 fmov x4, d2
 lsr x4, x4, x5
 cbnz x4, .Ltail
 ldr q0, [x2, 16]!
 cmeq v1.16b, v0.16b, 0
 shrn v2.8b, v1.8h, 4
 fmov x4, d2
 cbz x4, .Lstart_loop
 rbit x4, x4
 sub x5, x2, x1
 clz x4, x4
 add x4, x5, x4, lsr 2
 tbz x4, 4, .Lless16
 sub x5, x4, 15
 ldr q0, [x1]
 ldr q1, [x1, x5]
 str q0, [x0]
 str q1, [x0, x5]
 ret
.Ltail:
 rbit x4, x4
 clz x4, x4
 lsr x4, x4, 2
.Lless16:
 tbz x4, 3, .Lless8
 sub x5, x4, 7
 ldr x6, [x1]
 ldr x7, [x1, x5]
 str x6, [x0]
 str x7, [x0, x5]
 ret
 .p2align 4
.Lless8:
 subs x5, x4, 3
 b.lo .Lless4
 ldr w6, [x1]
 ldr w7, [x1, x5]
 str w6, [x0]
 str w7, [x0, x5]
 ret
.Lless4:
 cbz x4, .Lzerobyte
 ldrh w6, [x1]
 strh w6, [x0]
.Lzerobyte:
 strb wzr, [x0, x4]
 ret
 .p2align 4
.Lstart_loop:
 sub x5, x1, x0
 ldr q1, [x1]
 sub x3, x2, x5
 str q1, [x0]
.Lloop:
 str q0, [x3], 32
 ldr q0, [x2, 16]
 cmeq v1.16b, v0.16b, 0
 umaxp v2.16b, v1.16b, v1.16b
 fmov x4, d2
 cbnz x4, .Lloopend
 str q0, [x3, -16]
 ldr q0, [x2, 32]!
 cmeq v1.16b, v0.16b, 0
 umaxp v2.16b, v1.16b, v1.16b
 fmov x4, d2
 cbz x4, .Lloop
 add x3, x3, 16
.Lloopend:
 shrn v2.8b, v1.8h, 4 /* 128->64 */
 fmov x4, d2
 sub x3, x3, 31
 rbit x4, x4
 clz x4, x4
 lsr x4, x4, 2
 add x3, x3, x4
 ldr q0, [x3, x5]
 str q0, [x3]
 ret
.cfi_endproc; .size __raven_str_strcpy, .-__raven_str_strcpy;
