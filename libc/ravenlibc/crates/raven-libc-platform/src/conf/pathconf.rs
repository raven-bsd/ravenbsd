// SPDX-License-Identifier: BSD-2-Clause
/*
 * Copyright (C) 2008 The Android Open Source Project
 * All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 *  * Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 *  * Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in
 *    the documentation and/or other materials provided with the
 *    distribution.
 *
 * THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
 * "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
 * LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
 * FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
 * COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
 * INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
 * BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS
 * OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED
 * AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
 * OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT
 * OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from bionic:libc/bionic/pathconf.cpp @
// 731631f300090436d7f5df80d50b6275c8c60a93; the `_PC_*` dispatch and the
// pathconf/fpathconf split are kept; the `statfs.f_type` magic switches
// `__filesizebits` and `__link_max` and their uses are deleted because Raven
// `StatFs` has no `f_type`; `_PC_LINK_MAX` is the UFS 32000, `_PC_FILESIZEBITS`
// 64, `_PC_SYMLINK_MAX` -1, and `pathconf` stats the path first.

//! `pathconf`/`fpathconf` on Raven.

use raven_abi::errno::Errno;
use raven_libc_abi::{limits, structs::Statvfs, sysconf};

const LINK_MAX: i64 = 32000;
const MAX_CANON: i64 = 255;
const MAX_INPUT: i64 = 255;
const FILESIZEBITS: i64 = 64;
const POSIX_CHOWN_RESTRICTED: i64 = 1;
const POSIX_NO_TRUNC: i64 = 1;

/// Evaluate one `_PC_*` name against the filesystem of an open path.
///
/// `fs` may be `None` when the caller only needs the constants; the current implementation does
/// not use any `StatFs` field, but the parameter keeps the call site honest when PG-19 adds
/// `namemax` and `frsize`.
pub fn pathconf_value(name: i32, fs: Option<&Statvfs>) -> Result<i64, Errno> {
    let value = match name {
        sysconf::_PC_LINK_MAX => LINK_MAX,
        sysconf::_PC_MAX_CANON => MAX_CANON,
        sysconf::_PC_MAX_INPUT => MAX_INPUT,
        sysconf::_PC_NAME_MAX => i64::from(limits::NAME_MAX),
        sysconf::_PC_PATH_MAX => i64::from(limits::PATH_MAX),
        sysconf::_PC_PIPE_BUF => i64::from(limits::PIPE_BUF),
        sysconf::_PC_FILESIZEBITS => FILESIZEBITS,
        sysconf::_PC_CHOWN_RESTRICTED => POSIX_CHOWN_RESTRICTED,
        sysconf::_PC_NO_TRUNC => POSIX_NO_TRUNC,
        sysconf::_PC_VDISABLE => 0,
        sysconf::_PC_2_SYMLINKS => 1,
        sysconf::_PC_ALLOC_SIZE_MIN | sysconf::_PC_REC_XFER_ALIGN => fs
            .map_or(i64::from(raven_abi::PAGE_SIZE as i32), |fs| {
                fs.f_frsize as i64
            }),
        sysconf::_PC_REC_MIN_XFER_SIZE => fs.map_or(i64::from(raven_abi::PAGE_SIZE as i32), |fs| {
            fs.f_bsize as i64
        }),
        sysconf::_PC_ASYNC_IO
        | sysconf::_PC_PRIO_IO
        | sysconf::_PC_REC_INCR_XFER_SIZE
        | sysconf::_PC_REC_MAX_XFER_SIZE
        | sysconf::_PC_SYMLINK_MAX
        | sysconf::_PC_SYNC_IO
        | sysconf::_PC_SOCK_MAXBUF => -1,
        _ => return Err(Errno::EINVAL),
    };
    Ok(value)
}

/// `fpathconf` needs no path lookup.
pub fn fpathconf_value(name: i32, fs: &Statvfs) -> Result<i64, Errno> {
    pathconf_value(name, Some(fs))
}

#[cfg(test)]
#[path = "pathconf_ut.rs"]
mod tests;
