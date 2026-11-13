// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2006 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:lib/libc/secure/vsnprintf_chk.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! `__vsnprintf_chk` check core.

use super::ChkFail;

/// `len > slen` fails.
pub fn vsnprintf_chk(len: usize, slen: usize) -> Result<(), ChkFail> {
    if len > slen { Err(ChkFail) } else { Ok(()) }
}

#[cfg(test)]
#[path = "vsnprintf_chk_ut.rs"]
mod tests;
