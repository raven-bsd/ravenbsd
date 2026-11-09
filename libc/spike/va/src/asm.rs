// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The x86-64 SysV register-spill trampoline.

use core::arch::global_asm;

// System V AMD64 ABI: six integer argument registers and eight vector registers. The `va_list`
// save area layout is 48 bytes of GP registers followed by 128 bytes of 16-byte XMM slots.
//
// `my_snprintf` receives buf/n/fmt in rdi/rsi/rdx; the variadic arguments start at rcx. `al` is the
// number of vector registers used, so the XMM spill is skipped when it is zero. The overflow area
// begins after the return address (rbp+16 after the frame pointer push).
global_asm!(
    ".text",
    ".globl my_snprintf",
    ".type my_snprintf,@function",
    "my_snprintf:",
    "    push %rbp",
    "    mov %rsp, %rbp",
    "    sub $224, %rsp",
    "    mov %rdi, 0(%rsp)",
    "    mov %rsi, 8(%rsp)",
    "    mov %rdx, 16(%rsp)",
    "    mov %rcx, 24(%rsp)",
    "    mov %r8, 32(%rsp)",
    "    mov %r9, 40(%rsp)",
    "    test %al, %al",
    "    jz 1f",
    "    movups %xmm0, 48(%rsp)",
    "    movups %xmm1, 64(%rsp)",
    "    movups %xmm2, 80(%rsp)",
    "    movups %xmm3, 96(%rsp)",
    "    movups %xmm4, 112(%rsp)",
    "    movups %xmm5, 128(%rsp)",
    "    movups %xmm6, 144(%rsp)",
    "    movups %xmm7, 160(%rsp)",
    "1:",
    "    movl $24, 176(%rsp)",
    "    movl $48, 180(%rsp)",
    "    lea 16(%rbp), %rax",
    "    mov %rax, 184(%rsp)",
    "    lea 0(%rsp), %rax",
    "    mov %rax, 192(%rsp)",
    "    mov 0(%rsp), %rdi",
    "    mov 8(%rsp), %rsi",
    "    mov 16(%rsp), %rdx",
    "    lea 176(%rsp), %rcx",
    "    call raven_va_snprintf_impl",
    "    leave",
    "    ret",
    ".size my_snprintf, .-my_snprintf",
    options(att_syntax)
);

unsafe extern "C" {
    /// The variadic entry point implemented by the assembly trampoline above.
    ///
    /// # Safety
    ///
    /// `fmt` must be a valid NUL-terminated byte string and the variadic arguments must match its
    /// conversion specifiers. `buf` must point to `n` writable bytes when `n > 0`.
    pub fn my_snprintf(buf: *mut u8, n: usize, fmt: *const u8, ...) -> i32;
}
