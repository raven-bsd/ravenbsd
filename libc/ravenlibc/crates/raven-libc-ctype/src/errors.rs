// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Error values of the pure conversion logic. `errno` is the shell's job (guide 3.12).

/// The conversion failed; the shell stores `EILSEQ`.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum MbError {
    /// An invalid multibyte sequence or an unencodable code unit.
    Ilseq,
}

#[cfg(test)]
#[path = "errors_ut.rs"]
mod tests;
