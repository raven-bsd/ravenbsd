// SPDX-License-Identifier: BSD-3-Clause
/*
 * Copyright (c) 1993
 *	The Regents of the University of California.  All rights reserved.
 *
 * Redistribution and use in source and binary forms, with or without
 * modification, are permitted provided that the following conditions
 * are met:
 * 1. Redistributions of source code must retain the above copyright
 *    notice, this list of conditions and the following disclaimer.
 * 2. Redistributions in binary form must reproduce the above copyright
 *    notice, this list of conditions and the following disclaimer in the
 *    documentation and/or other materials provided with the distribution.
 * 3. Neither the name of the University nor the names of its contributors
 *    may be used to endorse or promote products derived from this software
 *    without specific prior written permission.
 *
 * THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
 * ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
 * IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
 * ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
 * FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
 * DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
 * OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
 * HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
 * LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
 * OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
 * SUCH DAMAGE.
 */
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// RavenBSD: transpiled from openbsd:lib/libc/gen/confstr.c @
// 4ce71f2817f675745b0596aaf04de4a122b1cc52; the `_CS_*` switch and the
// LP64 width tables are kept; `_CS_PATH` is the Raven single-root value
// `/bin:/usr/bin` (O-4), and the `_CS_POSIX_V7_THREADS_LDFLAGS` answer is the
// empty string because ravenlibc has no `libpthread` library.

//! `confstr`.

use raven_abi::errno::Errno;
use raven_libc_abi::sysconf;

const CS_PATH: &[u8] = b"/bin:/usr/bin";
const V6_WIDTH: &[u8] = b"POSIX_V6_LP64_OFF64\nPOSIX_V6_LPBIG_OFFBIG";
const V7_WIDTH: &[u8] = b"POSIX_V7_LP64_OFF64\nPOSIX_V7_LPBIG_OFFBIG";
const LIBC_VERSION: &[u8] = b"ravenlibc 0.1.0";
const LIBPTHREAD_VERSION: &[u8] = b"ravenlibc 0.1.0";

/// The value of a `_CS_*` name, without the NUL, or `None` for a zero-length answer.
pub fn value(name: i32) -> Result<Option<&'static [u8]>, Errno> {
    let value = match name {
        sysconf::_CS_PATH => Some(CS_PATH),
        sysconf::_CS_GNU_LIBC_VERSION => Some(LIBC_VERSION),
        sysconf::_CS_GNU_LIBPTHREAD_VERSION => Some(LIBPTHREAD_VERSION),
        sysconf::_CS_POSIX_V6_WIDTH_RESTRICTED_ENVS => Some(V6_WIDTH),
        sysconf::_CS_POSIX_V7_WIDTH_RESTRICTED_ENVS => Some(V7_WIDTH),
        // LP64 targets: these configuration strings are empty.
        sysconf::_CS_POSIX_V6_LP64_OFF64_CFLAGS
        | sysconf::_CS_POSIX_V6_LP64_OFF64_LDFLAGS
        | sysconf::_CS_POSIX_V6_LP64_OFF64_LIBS
        | sysconf::_CS_POSIX_V6_LPBIG_OFFBIG_CFLAGS
        | sysconf::_CS_POSIX_V6_LPBIG_OFFBIG_LDFLAGS
        | sysconf::_CS_POSIX_V6_LPBIG_OFFBIG_LIBS
        | sysconf::_CS_POSIX_V7_LP64_OFF64_CFLAGS
        | sysconf::_CS_POSIX_V7_LP64_OFF64_LDFLAGS
        | sysconf::_CS_POSIX_V7_LP64_OFF64_LIBS
        | sysconf::_CS_POSIX_V7_LPBIG_OFFBIG_CFLAGS
        | sysconf::_CS_POSIX_V7_LPBIG_OFFBIG_LDFLAGS
        | sysconf::_CS_POSIX_V7_LPBIG_OFFBIG_LIBS
        | sysconf::_CS_POSIX_V7_THREADS_CFLAGS
        | sysconf::_CS_V6_ENV
        | sysconf::_CS_V7_ENV => Some(&b""[..]),
        // ILP32-only programs have no configuration on an LP64 target.
        sysconf::_CS_POSIX_V6_ILP32_OFF32_CFLAGS
        | sysconf::_CS_POSIX_V6_ILP32_OFF32_LDFLAGS
        | sysconf::_CS_POSIX_V6_ILP32_OFF32_LIBS
        | sysconf::_CS_POSIX_V6_ILP32_OFFBIG_CFLAGS
        | sysconf::_CS_POSIX_V6_ILP32_OFFBIG_LDFLAGS
        | sysconf::_CS_POSIX_V6_ILP32_OFFBIG_LIBS
        | sysconf::_CS_POSIX_V7_ILP32_OFF32_CFLAGS
        | sysconf::_CS_POSIX_V7_ILP32_OFF32_LDFLAGS
        | sysconf::_CS_POSIX_V7_ILP32_OFF32_LIBS
        | sysconf::_CS_POSIX_V7_ILP32_OFFBIG_CFLAGS
        | sysconf::_CS_POSIX_V7_ILP32_OFFBIG_LDFLAGS
        | sysconf::_CS_POSIX_V7_ILP32_OFFBIG_LIBS => return Ok(None),
        sysconf::_CS_POSIX_V7_THREADS_LDFLAGS => Some(&b""[..]),
        _ => return Err(Errno::EINVAL),
    };
    Ok(value)
}

/// `confstr`: copy the value (truncating to `buf.len()`) and return the required size including
/// the NUL, or `0` for a name with no value.
pub fn confstr(name: i32, buf: Option<&mut [u8]>) -> Result<usize, Errno> {
    let value = value(name)?;
    let Some(value) = value else {
        return Ok(0);
    };
    let needed = value.len().saturating_add(1);
    if let Some(out) = buf {
        let copied = value.len().min(out.len());
        if let Some(slot) = out.get_mut(..copied) {
            slot.copy_from_slice(value.get(..copied).unwrap_or(&[]));
        }
        if out.len() > copied
            && let Some(slot) = out.get_mut(copied)
        {
            *slot = 0;
        }
    }
    Ok(needed)
}

#[cfg(test)]
#[path = "confstr_ut.rs"]
mod tests;
