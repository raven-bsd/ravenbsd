// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `libc-lint abi-freeze`: the offline frozen-symbol check (C68).

use std::path::Path;

use anyhow::Result;

use crate::libc_lint::Violation;

/// Add the frozen-record violations.
///
/// # Errors
/// An I/O error.
pub(crate) fn run(root: &Path, violations: &mut Vec<Violation>) -> Result<()> {
    violations.extend(crate::libc_abi::lint_frozen(root)?);
    Ok(())
}

#[cfg(test)]
#[path = "libc_lint_abi_ut.rs"]
mod tests;
