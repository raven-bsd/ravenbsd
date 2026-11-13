// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2006 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:lib/libc/secure/mempcpy_chk.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! `__mempcpy_chk` check core.

use super::ChkFail;
use crate::overlap::ssp_overlap;

/// `len > slen` or overlapping ranges fail.
pub fn mempcpy_chk(len: usize, slen: usize, dst: usize, src: usize) -> Result<(), ChkFail> {
    if len > slen {
        return Err(ChkFail);
    }
    if ssp_overlap(src, dst, len) {
        return Err(ChkFail);
    }
    Ok(())
}

#[cfg(test)]
#[path = "mempcpy_chk_ut.rs"]
mod tests;
