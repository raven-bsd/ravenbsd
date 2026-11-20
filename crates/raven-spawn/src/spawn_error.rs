// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `spawn` error type.
//!
//! `HasInterpreter` is the K06 name for a `PT_INTERP` request. PG-49 implements the interpreter
//! path, so `spawn` no longer returns it; the variant stays because `raven-init` (outside B71's
//! Owns) still matches it exhaustively. The next `raven-init` change can drop both the variant and
//! that match arm.

use raven_abi::errno::Errno;
use raven_elf::ElfError;

/// Why a spawn failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SpawnError {
    /// The path is not present in the initfs.
    NotFound,
    /// The path is not present on the file server.
    FileNotFound,
    /// The ELF image is invalid or cannot be loaded.
    BadElf(ElfError),
    /// Legacy K06 name for a `PT_INTERP` request; no longer returned by `spawn`.
    HasInterpreter,
    /// The stack image, an address or a file image exceeds its bound.
    TooLarge,
    /// The child working directory is empty, relative, contains NUL or is too long.
    BadCwd,
    /// A kernel syscall failed.
    Sys(Errno),
}
