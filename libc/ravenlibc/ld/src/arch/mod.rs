// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The loader-facing relocation tables. The formulas and the write engine are C52's
//! `raven_libc_elf::reloc::apply_object`; these modules name the architecture and keep the
//! loader's public vocabulary.

use raven_libc_elf::Arch;

/// The x86-64 table.
pub mod x86_64 {
    use super::*;
    use raven_libc_elf::RelocAction;
    use raven_libc_elf::reloc::classify;

    /// The architecture selector.
    pub const ARCH: Arch = Arch::X86_64;

    /// The action for one relocation number.
    pub fn reloc_action(r_type: u32) -> RelocAction {
        classify(ARCH, r_type)
    }
}

/// The AArch64 table.
pub mod aarch64 {
    use super::*;
    use raven_libc_elf::RelocAction;
    use raven_libc_elf::reloc::classify;

    /// The architecture selector.
    pub const ARCH: Arch = Arch::Aarch64;

    /// The action for one relocation number.
    pub fn reloc_action(r_type: u32) -> RelocAction {
        classify(ARCH, r_type)
    }
}

/// The RISC-V 64 table.
pub mod riscv64 {
    use super::*;
    use raven_libc_elf::RelocAction;
    use raven_libc_elf::reloc::classify;

    /// The architecture selector.
    pub const ARCH: Arch = Arch::Riscv64;

    /// The action for one relocation number.
    pub fn reloc_action(r_type: u32) -> RelocAction {
        classify(ARCH, r_type)
    }
}

/// The table of the running target.
#[must_use]
pub const fn target_arch() -> Arch {
    #[cfg(target_arch = "x86_64")]
    {
        Arch::X86_64
    }
    #[cfg(target_arch = "aarch64")]
    {
        Arch::Aarch64
    }
    #[cfg(target_arch = "riscv64")]
    {
        Arch::Riscv64
    }
}

#[cfg(test)]
#[path = "../arch_tables_ut.rs"]
mod tests;
