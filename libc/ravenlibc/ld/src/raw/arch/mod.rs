// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:libexec/rtld-elf/{amd64,aarch64,riscv}/rtld_start.S @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! Architecture entry stubs. The real register convention is C61's; these keep the
//! `_rtld_start` entry point and the `-e _rtld_start` link shape.

/// The `_rtld_start` entry assembly.
#[cfg(target_arch = "x86_64")]
pub mod entry {
    core::arch::global_asm!(
        ".text",
        ".globl _rtld_start",
        ".type _rtld_start,@function",
        "_rtld_start:",
        "jmp _rtld_main",
        ".size _rtld_start, .-_rtld_start",
        ".section .note.GNU-stack,\"\",@progbits",
    );
}

/// The `_rtld_start` entry assembly.
#[cfg(target_arch = "aarch64")]
pub mod entry {
    core::arch::global_asm!(
        ".text",
        ".globl _rtld_start",
        ".type _rtld_start,%function",
        "_rtld_start:",
        "b _rtld_main",
        ".size _rtld_start, .-_rtld_start",
        ".section .note.GNU-stack,\"\",%progbits",
    );
}

/// The `_rtld_start` entry assembly.
#[cfg(target_arch = "riscv64")]
pub mod entry {
    core::arch::global_asm!(
        ".text",
        ".globl _rtld_start",
        ".type _rtld_start,@function",
        "_rtld_start:",
        "j _rtld_main",
        ".size _rtld_start, .-_rtld_start",
        ".section .note.GNU-stack,\"\",@progbits",
    );
}

/// The TLS descriptor helpers are empty until the target TLS path lands.
pub mod tlsdesc {}
