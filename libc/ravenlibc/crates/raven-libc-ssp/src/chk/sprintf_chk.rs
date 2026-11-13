// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2006 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
//! `__sprintf_chk` post-format check core.

use super::ChkFail;

/// `slen > i32::MAX` skips the check; otherwise a non-negative `rv >= slen` fails.
pub fn sprintf_chk(rv: i32, slen: usize) -> Result<(), ChkFail> {
    if slen > i32::MAX as usize {
        return Ok(());
    }
    if rv >= 0 && rv as usize >= slen {
        return Err(ChkFail);
    }
    Ok(())
}

#[cfg(test)]
#[path = "sprintf_chk_ut.rs"]
mod tests;
