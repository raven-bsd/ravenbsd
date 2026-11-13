// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2006 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
//! `__strncat_chk` check core.

use super::{ChkFail, strlen};

/// `len == 0` returns immediately; `len > slen` fails. The byte-wise C accounting charges the
/// destination prefix, `min(strlen(src), len)` copied bytes and the final NUL.
pub fn strncat_chk(dst: &[u8], src: &[u8], len: usize, slen: usize) -> Result<(), ChkFail> {
    if len == 0 {
        return Ok(());
    }
    if len > slen {
        return Err(ChkFail);
    }
    let copied = strlen(src).min(len);
    let required = strlen(dst).wrapping_add(copied).wrapping_add(1);
    if required > slen {
        Err(ChkFail)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "strncat_chk_ut.rs"]
mod tests;
