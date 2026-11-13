// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2006 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
//! `__memcpy_chk` check core.

use super::ChkFail;
use crate::overlap::ssp_overlap;

/// `len > slen` or overlapping ranges fail.
pub fn memcpy_chk(len: usize, slen: usize, dst: usize, src: usize) -> Result<(), ChkFail> {
    if len > slen {
        return Err(ChkFail);
    }
    if ssp_overlap(src, dst, len) {
        return Err(ChkFail);
    }
    Ok(())
}

#[cfg(test)]
#[path = "memcpy_chk_ut.rs"]
mod tests;
