// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/{amd64,aarch64,riscv}/rtld_start.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! Architecture entry stubs. The kernel enters `_rtld_start` with the C8 stack pointer in
//! `%rsp`/`sp`; the stub passes it as the first argument and the loader later transfers to the
//! program entry with the same stack.

/// The `_rtld_start` entry assembly.
#[cfg(target_arch = "x86_64")]
pub mod entry {
    core::arch::global_asm!(
        ".text",
        ".globl _rtld_start",
        ".type _rtld_start,@function",
        "_rtld_start:",
        "mov rdi, rsp",
        "jmp _rtld_main",
        ".size _rtld_start, .-_rtld_start",
        // (sp: rdi, entry: rsi, rtld_fini: rdx). `Scrt1` reads the hook from `rdx`, so only
        // `rsp` moves before the indirect jump.
        ".globl __rl_rtld_transfer",
        ".type __rl_rtld_transfer,@function",
        "__rl_rtld_transfer:",
        "mov rax, rsi",
        "mov rsp, rdi",
        "xor ebp, ebp",
        "jmp rax",
        ".size __rl_rtld_transfer, .-__rl_rtld_transfer",
        ".section .note.GNU-stack,\"\",@progbits",
    );

    unsafe extern "C" {
        fn __rl_rtld_transfer(sp: usize, entry: usize, rtld_fini: usize) -> !;
    }

    /// Transfer control to `entry` with the original C8 stack and the loader exit hook.
    ///
    /// # Safety
    /// `sp` must be the untouched start stack and `entry` a relocated executable entry.
    pub unsafe fn enter(sp: usize, entry: usize, rtld_fini: usize) -> ! {
        // SAFETY: the caller guarantees the stack and the entry.
        unsafe { __rl_rtld_transfer(sp, entry, rtld_fini) }
    }
}

/// The `_rtld_start` entry assembly.
#[cfg(target_arch = "aarch64")]
pub mod entry {
    core::arch::global_asm!(
        ".text",
        ".globl _rtld_start",
        ".type _rtld_start,%function",
        "_rtld_start:",
        "mov x0, sp",
        "b _rtld_main",
        ".size _rtld_start, .-_rtld_start",
        // (sp: x0, entry: x1, rtld_fini: x2); `Scrt1` reads x1 as the hook.
        ".globl __rl_rtld_transfer",
        ".type __rl_rtld_transfer,%function",
        "__rl_rtld_transfer:",
        "mov x3, x1",
        "mov x1, x2",
        "mov sp, x0",
        "br x3",
        ".size __rl_rtld_transfer, .-__rl_rtld_transfer",
        ".section .note.GNU-stack,\"\",%progbits",
    );

    unsafe extern "C" {
        fn __rl_rtld_transfer(sp: usize, entry: usize, rtld_fini: usize) -> !;
    }

    /// Transfer control to `entry` with the original C8 stack and the loader exit hook.
    ///
    /// # Safety
    /// `sp` must be the untouched start stack and `entry` a relocated executable entry.
    pub unsafe fn enter(sp: usize, entry: usize, rtld_fini: usize) -> ! {
        // SAFETY: the caller guarantees the stack and the entry.
        unsafe { __rl_rtld_transfer(sp, entry, rtld_fini) }
    }
}

/// The `_rtld_start` entry assembly.
#[cfg(target_arch = "riscv64")]
pub mod entry {
    core::arch::global_asm!(
        ".text",
        ".globl _rtld_start",
        ".type _rtld_start,@function",
        "_rtld_start:",
        "mv a0, sp",
        "j _rtld_main",
        ".size _rtld_start, .-_rtld_start",
        // (sp: a0, entry: a1, rtld_fini: a2); `Scrt1` reads a1 as the hook.
        ".globl __rl_rtld_transfer",
        ".type __rl_rtld_transfer,@function",
        "__rl_rtld_transfer:",
        "mv a3, a1",
        "mv a1, a2",
        "mv sp, a0",
        "jr a3",
        ".size __rl_rtld_transfer, .-__rl_rtld_transfer",
        ".section .note.GNU-stack,\"\",@progbits",
    );

    unsafe extern "C" {
        fn __rl_rtld_transfer(sp: usize, entry: usize, rtld_fini: usize) -> !;
    }

    /// Transfer control to `entry` with the original C8 stack and the loader exit hook.
    ///
    /// # Safety
    /// `sp` must be the untouched start stack and `entry` a relocated executable entry.
    pub unsafe fn enter(sp: usize, entry: usize, rtld_fini: usize) -> ! {
        // SAFETY: the caller guarantees the stack and the entry.
        unsafe { __rl_rtld_transfer(sp, entry, rtld_fini) }
    }
}

/// The TLS descriptor helpers are empty until the target TLS path lands.
pub mod tlsdesc {}
