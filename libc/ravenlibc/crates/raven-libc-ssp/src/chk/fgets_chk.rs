// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2006 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:lib/libc/secure/fgets_chk.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! `__fgets_chk` check core.

use super::ChkFail;

/// `slen >= i32::MAX` skips the check; otherwise `len >= 0 && len as usize > slen` fails.
pub fn fgets_chk(len: i32, slen: usize) -> Result<(), ChkFail> {
    if slen >= i32::MAX as usize {
        return Ok(());
    }
    if len >= 0 && len as usize > slen {
        return Err(ChkFail);
    }
    Ok(())
}

#[cfg(test)]
#[path = "fgets_chk_ut.rs"]
mod tests;
