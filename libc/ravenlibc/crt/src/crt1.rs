// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//!
//! `crt1.o`: the `_start` of a static, non-PIE program.
//!
//! CLEANROOM. The instruction bodies are the three `_start` sequences of
//! `userland/rt/src/entry.rs:14-41` (our own code), extended with the four-argument convention
//! of C61 4.2: `sp0`, `rtld_fini` (0 here), and the static `IRELATIVE` range that `ld.lld`
//! defines as `__rela_iplt_start`/`__rela_iplt_end` in a static link.
//!
//! The frame pointer (and the link register on AArch64 and RISC-V) is cleared, the C8 stack
//! pointer is saved before the 16-byte alignment, `__raven_libc_start` is called, and a trap
//! follows because it never returns.

#[cfg(target_arch = "x86_64")]
core::arch::global_asm!(
    note_raven_abi!(),
    ".text",
    ".globl _start",
    ".type _start,@function",
    "_start:",
    "xor %ebp,%ebp",
    "mov %rsp,%rdi",
    "xor %esi,%esi",
    ".weak __rela_iplt_start",
    ".weak __rela_iplt_end",
    "lea __rela_iplt_start(%rip),%rdx",
    "lea __rela_iplt_end(%rip),%rcx",
    "and $-16,%rsp",
    "call __raven_libc_start",
    "ud2",
    ".size _start,.-_start",
    ".globl __raven_crt_kind_crt1",
    "__raven_crt_kind_crt1 = .",
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
    "mov x1, #0",
    ".weak __rela_iplt_start",
    ".weak __rela_iplt_end",
    "adrp x2, __rela_iplt_start",
    "add x2, x2, :lo12:__rela_iplt_start",
    "adrp x3, __rela_iplt_end",
    "add x3, x3, :lo12:__rela_iplt_end",
    "and sp, x0, #-16",
    "bl __raven_libc_start",
    "brk #0",
    ".globl __raven_crt_kind_crt1",
    "__raven_crt_kind_crt1 = .",
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
    "li a1, 0",
    ".weak __rela_iplt_start",
    ".weak __rela_iplt_end",
    "lla a2, __rela_iplt_start",
    "lla a3, __rela_iplt_end",
    "andi sp, sp, -16",
    "call __raven_libc_start",
    "unimp",
    ".globl __raven_crt_kind_crt1",
    "__raven_crt_kind_crt1 = .",
    ".section .note.GNU-stack,\"\",%progbits",
);
