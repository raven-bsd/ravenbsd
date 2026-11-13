// SPDX-License-Identifier: ISC
// Copyright (c) 2003, 2004 Todd C. Miller <Todd.Miller@courtesan.com>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/gen/ftw.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: $OpenBSD: ftw.c,v 1.5 2005/08/08 08:05:34 espie Exp $)
//
// Permission to use, copy, modify, and distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
// WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
// MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
// ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
// WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
// ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
// OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.
//
// Sponsored in part by the Defense Advanced Research Projects
// Agency (DARPA) and Air Force Research Laboratory, Air Force
// Materiel Command, USAF, under agreement number F39502-99-1-0512.

//! `struct FTW` and the `ftw` walk (COPY of `freebsd:lib/libc/gen/ftw.c`, ISC).
//!
//! `ftw` is `fts_open(FTS_COMFOLLOW|FTS_NOCHDIR|FTS_LOGICAL)` plus the `FTS_*` to `FTW_*`
//! mapping. `FTS_DC` is fatal here (the donor sets `ELOOP`); `nftw` skips it instead.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EINVAL, ELOOP};
use raven_libc_abi::generated::structs::Stat;

use crate::DirSys;
use crate::consts::*;
use crate::fts::fts_open;

/// `FTW_F`: a regular file.
pub const FTW_F: i32 = 0;
/// `FTW_D`: a directory visited in pre-order.
pub const FTW_D: i32 = 1;
/// `FTW_DNR`: an unreadable directory.
pub const FTW_DNR: i32 = 2;
/// `FTW_DP`: a directory visited in post-order.
pub const FTW_DP: i32 = 3;
/// `FTW_NS`: `stat` failed.
pub const FTW_NS: i32 = 4;
/// `FTW_SL`: a symbolic link.
pub const FTW_SL: i32 = 5;
/// `FTW_SLN`: a symbolic link without a target.
pub const FTW_SLN: i32 = 6;

/// `struct FTW`: the base offset and level of the current name.
#[derive(Clone, Copy, Debug, Default)]
pub struct Ftw {
    /// `base = fts_pathlen - fts_namelen` (do not recompute).
    pub base: usize,
    /// `level = fts_level`.
    pub level: i64,
}

/// The `ftw` callback: path, stat, type.
pub type FtwFunc<'a> = &'a mut dyn FnMut(&[u8], &Stat, i32) -> i32;

/// Walk `path` and call `func` for every node in pre-order.
///
/// # Errors
///
/// `EINVAL` for `nfds < 1`, the `fts_open` errors, or `ELOOP` on a cycle.
pub fn ftw<S: DirSys>(
    sys: &mut S,
    path: &[u8],
    func: FtwFunc<'_>,
    nfds: i32,
) -> Result<i32, Errno> {
    // XXX - nfds is currently unused (the donor's comment).
    if nfds < 1 {
        return Err(Errno(EINVAL as u16));
    }
    let mut fts = fts_open(
        sys,
        &[path],
        FTS_LOGICAL | FTS_COMFOLLOW | FTS_NOCHDIR,
        None,
    )?;
    let mut error = 0;
    while let Some(cur) = fts.read(sys)? {
        let flag = match cur.info {
            FTS_D => FTW_D,
            FTS_DNR => FTW_DNR,
            // We only visit in pre-order.
            FTS_DP => continue,
            FTS_F | FTS_DEFAULT => FTW_F,
            FTS_NS | FTS_NSOK | FTS_SLNONE => FTW_NS,
            FTS_SL => FTW_SL,
            // RavenBSD: the donor sets ELOOP and aborts ftw on a cycle (nftw skips it).
            FTS_DC => {
                let _ = fts.close(sys);
                return Err(Errno(ELOOP as u16));
            }
            _ => {
                error = -1;
                break;
            }
        };
        let stat = cur.statp.unwrap_or(Stat::ZERO);
        error = func(&cur.path, &stat, flag);
        if error != 0 {
            break;
        }
    }
    let _ = fts.close(sys);
    Ok(error)
}
