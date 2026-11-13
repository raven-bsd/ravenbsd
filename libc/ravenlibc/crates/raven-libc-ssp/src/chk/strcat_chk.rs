// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2006 The NetBSD Foundation, Inc.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
//! `__strcat_chk` check core.

use super::{ChkFail, strlen};

/// The C loop charges one byte for every existing destination character, one for every source
/// character and one for the final NUL.
pub fn strcat_chk(dst: &[u8], src: &[u8], slen: usize) -> Result<(), ChkFail> {
    let required = strlen(dst).wrapping_add(strlen(src)).wrapping_add(1);
    if required > slen {
        Err(ChkFail)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "strcat_chk_ut.rs"]
mod tests;
