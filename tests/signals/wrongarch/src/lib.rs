// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host test crate for the S710b wrong-architecture artifact rejection row.
//!
//! The test reads the built x86_64 signal fixture and asks the real `raven-elf` parser for an
//! AArch64 image. The loader's exact rejection (`ElfError::BadMachine`) must fire before any
//! segment is mapped.

#[cfg(test)]
#[path = "wrongarch_ut.rs"]
mod tests;
