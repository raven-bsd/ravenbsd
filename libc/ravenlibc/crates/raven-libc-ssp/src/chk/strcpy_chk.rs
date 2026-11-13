// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2006 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:lib/libc/secure/strcpy_chk.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! `__strcpy_chk` check core.

use super::{ChkFail, strlen};
use crate::overlap::ssp_overlap;

/// Returns `strlen(src) + 1`; fails when that exceeds `slen` or on overlap.
pub fn strcpy_chk(src: &[u8], slen: usize, dst: usize, src_addr: usize) -> Result<usize, ChkFail> {
    let len = strlen(src).wrapping_add(1);
    if len > slen {
        return Err(ChkFail);
    }
    if ssp_overlap(src_addr, dst, len) {
        return Err(ChkFail);
    }
    Ok(len)
}

#[cfg(test)]
#[path = "strcpy_chk_ut.rs"]
mod tests;
