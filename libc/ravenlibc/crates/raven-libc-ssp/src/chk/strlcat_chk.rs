// SPDX-License-Identifier: ISC
// Copyright (c) 1998, 2015 Todd C. Miller <Todd.Miller@courtesan.com>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from freebsd:lib/libc/secure/strlcat_chk.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
//! `__strlcat_chk` check core.

use super::{ChkFail, strlen};

/// The donor's `dsize`/`dbufsize` accounting, transcribed literally.
pub fn strlcat_chk(
    dst: &[u8],
    src: &[u8],
    dsize: usize,
    dbufsize: usize,
) -> Result<usize, ChkFail> {
    if dsize > dbufsize {
        return Err(ChkFail);
    }
    let dst_len = strlen(dst).min(dsize);
    let mut n = dsize.wrapping_sub(dst_len);
    if n == 0 {
        return Ok(dst_len.wrapping_add(strlen(src)));
    }
    n = n.wrapping_sub(1);
    let mut budget = dbufsize;
    let mut copied = 0usize;
    let src_len = strlen(src);
    while copied < src_len {
        if n != 0 {
            if budget == 0 {
                return Err(ChkFail);
            }
            budget = budget.wrapping_sub(1);
            n = n.wrapping_sub(1);
        }
        copied = copied.wrapping_add(1);
    }
    if budget == 0 {
        return Err(ChkFail);
    }
    Ok(dst_len.wrapping_add(src_len))
}

#[cfg(test)]
#[path = "strlcat_chk_ut.rs"]
mod tests;
