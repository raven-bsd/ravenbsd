// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Errors returned by the image planner and stack builder.

use core::fmt;

/// A process-image setup failure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinuxElfError {
    /// The initial stack would exceed the 1 MiB limit.
    TooLarge,
    /// An argv, envp or execfn byte string contains an interior NUL.
    EmbeddedNul,
    /// The main executable declares `PT_INTERP` but no interpreter was supplied.
    MissingInterpreter,
    /// The supplied interpreter is not `ET_DYN`.
    InterpreterNotDynamic,
    /// An interpreter was supplied but the main executable declares no `PT_INTERP`.
    UnexpectedInterpreter,
    /// The ELF header or program-header table cannot be read through `raven-elf`'s accessor.
    BadProgramHeaders,
    /// A page size or load base is zero or not page aligned.
    InvalidConfig,
    /// An address calculation overflowed `u64`.
    AddressOverflow,
}

impl fmt::Display for LinuxElfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            Self::TooLarge => "initial stack exceeds 1 MiB",
            Self::EmbeddedNul => "argument or environment string contains NUL",
            Self::MissingInterpreter => "PT_INTERP requires an interpreter ELF",
            Self::InterpreterNotDynamic => "interpreter is not ET_DYN",
            Self::UnexpectedInterpreter => "interpreter supplied without PT_INTERP",
            Self::BadProgramHeaders => "cannot read the ELF program-header table",
            Self::InvalidConfig => "invalid page size or load base",
            Self::AddressOverflow => "address calculation overflowed",
        };
        f.write_str(text)
    }
}

impl core::error::Error for LinuxElfError {}
