// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The target architecture of the ABI.

/// One supported architecture.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Arch {
    /// x86-64.
    X86_64,
    /// AArch64.
    Aarch64,
    /// RISC-V 64.
    Riscv64,
}

impl Arch {
    /// Every supported architecture.
    pub const ALL: [Arch; 3] = [Arch::X86_64, Arch::Aarch64, Arch::Riscv64];

    /// The architecture this crate was built for.
    #[cfg(target_arch = "x86_64")]
    pub const CURRENT: Arch = Arch::X86_64;
    /// The architecture this crate was built for.
    #[cfg(target_arch = "aarch64")]
    pub const CURRENT: Arch = Arch::Aarch64;
    /// The architecture this crate was built for.
    #[cfg(target_arch = "riscv64")]
    pub const CURRENT: Arch = Arch::Riscv64;

    /// The `uname -m` and table name of the architecture.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::Riscv64 => "riscv64",
        }
    }

    /// The Rust target triple used by the acceptance builds.
    #[must_use]
    pub const fn rust_target(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64-unknown-none",
            Self::Aarch64 => "aarch64-unknown-none-softfloat",
            Self::Riscv64 => "riscv64gc-unknown-none-elf",
        }
    }

    /// Parse a table architecture name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        match name {
            "x86_64" => Some(Self::X86_64),
            "aarch64" => Some(Self::Aarch64),
            "riscv64" => Some(Self::Riscv64),
            _ => None,
        }
    }
}
