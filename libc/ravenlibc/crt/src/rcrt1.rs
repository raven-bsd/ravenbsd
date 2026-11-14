// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:crt/rcrt1.c and
// musl:arch/{x86_64,aarch64,riscv64}/crt_arch.h
// @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (donor files have no per-file header; covered by musl/COPYRIGHT)
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! `rcrt1.o`: the `_start` of a static-PIE program and its stage-1 relocation.
//!
//! The entry assembly is the musl `crt_arch.h` text for the three architectures. The donor's
//! `__dls2` (which calls `__libc_start_main`) is replaced by
//! [`__raven_rcrt1_stage1`]: it runs [`crate::selfreloc::relocate_self`] and then enters the
//! one start function of C61 4.5 with `rtld_fini = 0` and an empty `IRELATIVE` range (the
//! self-relocator has already applied it).
//!
//! Nothing here may touch a `static`, a string constant or a function pointer before
//! `relocate_self` returns (`A-ld-1`); the two arguments and their uses are the whole body.

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    note_raven_abi!(),
    ".text",
    ".globl _start",
    ".type _start,@function",
    "_start:",
    "xor %rbp,%rbp",
    "mov %rsp,%rdi",
    ".weak _DYNAMIC",
    ".hidden _DYNAMIC",
    "lea _DYNAMIC(%rip),%rsi",
    "andq $-16,%rsp",
    "call __raven_rcrt1_stage1",
    "mov %rax,%rdi",
    "xor %esi,%esi",
    "xor %edx,%edx",
    "xor %ecx,%ecx",
    "call __raven_libc_start",
    "ud2",
    ".size _start,.-_start",
    ".globl __raven_crt_kind_rcrt1",
    "__raven_crt_kind_rcrt1 = .",
    ".section .note.GNU-stack,\"\",%progbits",
    options(att_syntax),
);

#[cfg(target_arch = "aarch64")]
core::arch::global_asm!(
    note_raven_abi!(),
    ".text",
    ".globl _start",
    ".type _start,%function",
    "_start:",
    "mov x29, #0",
    "mov x30, #0",
    "mov x0, sp",
    ".weak _DYNAMIC",
    ".hidden _DYNAMIC",
    "adrp x1, _DYNAMIC",
    "add x1, x1, #:lo12:_DYNAMIC",
    "and sp, x0, #-16",
    "bl __raven_rcrt1_stage1",
    "mov x1, #0",
    "mov x2, #0",
    "mov x3, #0",
    "bl __raven_libc_start",
    "brk #0",
    ".globl __raven_crt_kind_rcrt1",
    "__raven_crt_kind_rcrt1 = .",
    ".section .note.GNU-stack,\"\",%progbits",
);

#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(
    note_raven_abi!(),
    ".section .sdata,\"aw\"",
    ".text",
    ".globl _start",
    ".type _start,%function",
    "_start:",
    ".weak __global_pointer$",
    ".hidden __global_pointer$",
    ".option push",
    ".option norelax",
    "lla gp, __global_pointer$",
    ".option pop",
    "li fp, 0",
    "li ra, 0",
    "mv a0, sp",
    ".weak _DYNAMIC",
    ".hidden _DYNAMIC",
    "lla a1, _DYNAMIC",
    "andi sp, sp, -16",
    "call __raven_rcrt1_stage1",
    "li a1, 0",
    "li a2, 0",
    "li a3, 0",
    "call __raven_libc_start",
    "unimp",
    ".globl __raven_crt_kind_rcrt1",
    "__raven_crt_kind_rcrt1 = .",
    ".section .note.GNU-stack,\"\",%progbits",
);

/// Stage 1 of a static-PIE program: relocate this image and hand `sp0` back to the entry asm.
///
/// The assembly above is the only caller. It passes the C8 stack pointer and this image's
/// terminated `_DYNAMIC`, gets `sp0` back in the first return register and enters
/// `__raven_libc_start` with a direct call. The split keeps the call to libc out of the Rust
/// body: a Rust call to an external symbol of a PIC object is a GOT reference, and a GOT slot
/// is only valid after `relocate_self` has run (`A-ld-1`). The assembly call is PC-relative.
///
/// # Safety
///
/// `sp` must be the C8 stack pointer and `dynv` this image's terminated `_DYNAMIC`. Only the
/// assembly entry above calls this, with exactly those values.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __raven_rcrt1_stage1(sp: *const usize, dynv: *const usize) -> usize {
    // SAFETY: the entry assembly passes the C8 stack and this image's `_DYNAMIC`, and every
    // relocation offset was produced by the linker for this image.
    let _base = unsafe { crate::selfreloc::relocate_self(sp, dynv) };
    sp as usize
}
