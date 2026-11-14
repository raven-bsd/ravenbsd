// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `Scrt1.o`: the `_start` of a dynamic PIE program.
//!
//! CLEANROOM, the same instruction bodies as [`crate::crt1`] with the loader convention of
//! `donor-map/13` 3.4 step 17: the loader jumps to the entry with `sp0` in the first argument
//! register **and** on the stack, and with its exit hook in x86-64 `rdx`, AArch64 `x1`, RISC-V
//! `a1`. The static `IRELATIVE` range is `0, 0`: the loader already applied it.
//!
//! On x86-64 the hook word is copied to `rsi` before `rdx` is zeroed; the copy is the first use
//! of `rdx` and no instruction writes `rdx` before it.

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    note_raven_abi!(),
    ".text",
    ".globl _start",
    ".type _start,@function",
    "_start:",
    "xor %ebp,%ebp",
    "mov %rsp,%rdi",
    "mov %rdx,%rsi",
    "xor %edx,%edx",
    "xor %ecx,%ecx",
    "and $-16,%rsp",
    "call __raven_libc_start",
    "ud2",
    ".size _start,.-_start",
    ".globl __raven_crt_kind_scrt1",
    "__raven_crt_kind_scrt1 = .",
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
    "mov x2, #0",
    "mov x3, #0",
    "and sp, x0, #-16",
    "bl __raven_libc_start",
    "brk #0",
    ".globl __raven_crt_kind_scrt1",
    "__raven_crt_kind_scrt1 = .",
    ".section .note.GNU-stack,\"\",%progbits",
);

#[cfg(target_arch = "riscv64")]
core::arch::global_asm!(
    note_raven_abi!(),
    ".text",
    ".globl _start",
    ".type _start,%function",
    "_start:",
    ".weak __global_pointer$",
    ".option push",
    ".option norelax",
    "lla gp, __global_pointer$",
    ".option pop",
    "li fp, 0",
    "li ra, 0",
    "mv a0, sp",
    "li a2, 0",
    "li a3, 0",
    "andi sp, sp, -16",
    "call __raven_libc_start",
    "unimp",
    ".globl __raven_crt_kind_scrt1",
    "__raven_crt_kind_scrt1 = .",
    ".section .note.GNU-stack,\"\",%progbits",
);
