// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2006 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:lib/libc/secure/strlcpy_chk.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! `__strlcpy_chk` check core.

use super::{ChkFail, strlen};

/// `dsize > dbufsize` fails; returns the `strlcpy` result (`strlen(src)`).
pub fn strlcpy_chk(src: &[u8], dsize: usize, dbufsize: usize) -> Result<usize, ChkFail> {
    if dsize > dbufsize {
        return Err(ChkFail);
    }
    Ok(strlen(src))
}

#[cfg(test)]
#[path = "strlcpy_chk_ut.rs"]
mod tests;
