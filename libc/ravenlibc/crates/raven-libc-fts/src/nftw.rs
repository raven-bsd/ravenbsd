// SPDX-License-Identifier: ISC
// Copyright (c) 2003, 2004 Todd C. Miller <Todd.Miller@courtesan.com>
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from freebsd:lib/libc/gen/nftw.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: $OpenBSD: nftw.c,v 1.7 2006/03/31 19:41:44 millert Exp $)
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

//! `nftw` (COPY of `freebsd:lib/libc/gen/nftw.c`, ISC).
//!
//! `FTW_CHDIR` is a no-op because H-FTS-1 forces `FTS_NOCHDIR`: the callback always sees a path
//! relative to the starting working directory. `FTS_DC` is skipped, not fatal (the FreeBSD
//! behaviour; OpenBSD aborts with `ELOOP`, which is not copied).

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::EINVAL;
use raven_libc_abi::generated::structs::Stat;

use crate::DirSys;
use crate::consts::*;
use crate::fts::fts_open;
use crate::ftw::{FTW_D, FTW_DNR, FTW_DP, FTW_F, FTW_NS, FTW_SL, FTW_SLN, Ftw};

/// `FTW_PHYS`: physical walk, do not follow symbolic links.
pub const FTW_PHYS: i32 = 0x01;
/// `FTW_MOUNT`: do not cross a mount point.
pub const FTW_MOUNT: i32 = 0x02;
/// `FTW_DEPTH`: post-order (subdirectories before the directory).
pub const FTW_DEPTH: i32 = 0x04;
/// `FTW_CHDIR`: change into a directory before reading it (a no-op under forced `FTS_NOCHDIR`).
pub const FTW_CHDIR: i32 = 0x08;

/// The `nftw` callback: path, stat, type, [`Ftw`].
pub type NftwFunc<'a> = &'a mut dyn FnMut(&[u8], &Stat, i32, &Ftw) -> i32;

/// Walk `path`, calling `func` in pre-order or post-order (`FTW_DEPTH`).
///
/// # Errors
///
/// `EINVAL` for `nfds < 1` or the `fts_open` errors.
pub fn nftw<S: DirSys>(
    sys: &mut S,
    path: &[u8],
    func: NftwFunc<'_>,
    nfds: i32,
    ftwflags: i32,
) -> Result<i32, Errno> {
    // XXX - nfds is currently unused (the donor's comment).
    if nfds < 1 {
        return Err(Errno(EINVAL as u16));
    }
    let mut ftsflags = FTS_COMFOLLOW;
    if ftwflags & FTW_CHDIR == 0 {
        ftsflags |= FTS_NOCHDIR;
    }
    if ftwflags & FTW_MOUNT != 0 {
        ftsflags |= FTS_XDEV;
    }
    if ftwflags & FTW_PHYS != 0 {
        ftsflags |= FTS_PHYSICAL;
    } else {
        ftsflags |= FTS_LOGICAL;
    }
    let postorder = ftwflags & FTW_DEPTH != 0;
    let mut fts = fts_open(sys, &[path], ftsflags, None)?;
    let mut error = 0;
    while let Some(cur) = fts.read(sys)? {
        let flag = match cur.info {
            FTS_D => {
                if postorder {
                    continue;
                }
                FTW_D
            }
            // RavenBSD: FreeBSD skips a cycle; OpenBSD aborts with ELOOP (doc 07 4.5).
            FTS_DC => continue,
            FTS_DNR => FTW_DNR,
            FTS_DP => {
                if !postorder {
                    continue;
                }
                FTW_DP
            }
            FTS_F | FTS_DEFAULT => FTW_F,
            FTS_NS | FTS_NSOK => FTW_NS,
            FTS_SL => FTW_SL,
            FTS_SLNONE => FTW_SLN,
            _ => {
                error = -1;
                break;
            }
        };
        let ftw = Ftw {
            // Do not recompute: the donor uses fts_pathlen - fts_namelen.
            base: cur.path.len().saturating_sub(cur.name.len()),
            level: cur.level,
        };
        let stat = cur.statp.unwrap_or(Stat::ZERO);
        error = func(&cur.path, &stat, flag, &ftw);
        if error != 0 {
            break;
        }
    }
    let _ = fts.close(sys);
    Ok(error)
}
