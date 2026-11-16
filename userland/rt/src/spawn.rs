// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Create a child process from an ELF file in the initfs or on a file server.
//!
//! P9a moved the loader into the shared `raven-spawn` crate so `raven-rt` and ravenlibc use one
//! planning, mapping and stack-image implementation. This module keeps the runtime's public
//! `raven_rt::spawn` surface as a re-export.
//!
//! The re-export still includes the PG-49 `PT_INTERP` mapping and the PG-50 auxiliary vector
//! (`AT_BASE` for an `ET_DYN` interpreter, `AT_PHDR`/`AT_PHNUM` for the main image); the code
//! lives in `crates/raven-spawn/src/lib.rs`.

pub use raven_spawn::*;

/// The pre-P9a runtime error enum, kept until its module is removed with the next
/// `raven-init` change. New code uses [`SpawnError`].
#[doc(hidden)]
pub type LegacySpawnError = crate::spawn_error::SpawnError;
