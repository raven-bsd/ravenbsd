// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-architecture raw system-call stubs and the register-convention table.
//!
//! The six functions are the only `asm!` system-call instructions in ravenlibc. The fake kernel
//! replaces them at compile time when `host-fake` is enabled.

/// The register convention of one architecture, as asserted by the golden test.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Convention {
    /// `uname -m` style name.
    pub arch: &'static str,
    /// The user instruction.
    pub instruction: &'static str,
    /// Register holding the syscall number.
    pub nr_reg: &'static str,
    /// Registers holding arguments 0..5.
    pub arg_regs: [&'static str; 6],
    /// Register holding the result.
    pub ret_reg: &'static str,
}

/// The three conventions fixed by `architecture/01-kernel.md` section 3.
pub mod convention {
    use super::Convention;

    /// x86-64 `syscall`.
    pub const X86_64: Convention = Convention {
        arch: "x86_64",
        instruction: "syscall",
        nr_reg: "rax",
        arg_regs: ["rdi", "rsi", "rdx", "r10", "r8", "r9"],
        ret_reg: "rax",
    };
    /// AArch64 `svc #0`.
    pub const AARCH64: Convention = Convention {
        arch: "aarch64",
        instruction: "svc #0",
        nr_reg: "x8",
        arg_regs: ["x0", "x1", "x2", "x3", "x4", "x5"],
        ret_reg: "x0",
    };
    /// RISC-V 64 `ecall`.
    pub const RISCV64: Convention = Convention {
        arch: "riscv64",
        instruction: "ecall",
        nr_reg: "a7",
        arg_regs: ["a0", "a1", "a2", "a3", "a4", "a5"],
        ret_reg: "a0",
    };

    /// Every convention, in a target-independent order.
    pub const ALL: [Convention; 3] = [X86_64, AARCH64, RISCV64];
}

#[cfg(all(target_arch = "aarch64", not(feature = "host-fake")))]
pub mod aarch64;
#[cfg(all(target_arch = "riscv64", not(feature = "host-fake")))]
pub mod riscv64;
#[cfg(all(target_arch = "x86_64", not(feature = "host-fake")))]
pub mod x86_64;

#[cfg(all(target_arch = "aarch64", not(feature = "host-fake")))]
pub use aarch64::{syscall0, syscall1, syscall2, syscall3, syscall4, syscall5, syscall6};
#[cfg(all(target_arch = "riscv64", not(feature = "host-fake")))]
pub use riscv64::{syscall0, syscall1, syscall2, syscall3, syscall4, syscall5, syscall6};
#[cfg(all(target_arch = "x86_64", not(feature = "host-fake")))]
pub use x86_64::{syscall0, syscall1, syscall2, syscall3, syscall4, syscall5, syscall6};

#[cfg(feature = "host-fake")]
pub use crate::fake::syscall::{
    syscall0, syscall1, syscall2, syscall3, syscall4, syscall5, syscall6,
};

#[cfg(test)]
#[path = "arch_ut.rs"]
mod tests;
