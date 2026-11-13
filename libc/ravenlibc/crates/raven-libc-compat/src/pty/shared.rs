// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1990, 1993, 1994 The Regents of the University of California.  All rights reserved.
//
// RavenBSD: transpiled from freebsd:lib/libutil/pty.c and freebsd:lib/libutil/login_tty.c
// @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tags: $FreeBSD$; the FreeBSD files carry the SPDX line BSD-3-Clause)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The shared constants and helpers of the `openpty` family.

use raven_libc_abi::errno::Errno;

/// `O_RDWR`.
pub(crate) const O_RDWR: i32 = 0o2;
/// `O_NOCTTY`.
pub(crate) const O_NOCTTY: i32 = 0o400;
/// The `ptsname_r` buffer used here (the donor's `strcpy` target is caller-owned).
pub(crate) const PTNAME_CAP: usize = 128;

pub(crate) fn errno_of(value: i32) -> Errno {
    Errno(value as u16)
}

pub(crate) fn copy_name(dst: &mut [u8], source: &[u8]) -> Result<(), Errno> {
    if dst.len() < source.len().wrapping_add(1) {
        return Err(errno_of(raven_libc_abi::errno::ERANGE));
    }
    dst[..source.len()].copy_from_slice(source);
    dst[source.len()] = 0;
    Ok(())
}
