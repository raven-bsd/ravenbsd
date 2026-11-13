// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strmode.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: strmode.c,v 1.9 2024/06/23 07:08:26 otto Exp $)
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

//! The `ls -l` mode string (`strmode`).
//!
//! RavenBSD: `S_IFWHT` is dropped (BSD-only; Linux `S_IFMT` has no value for it) and the
//! constants come from `raven-libc-abi` (Linux values equal the BSD octal values).

use raven_libc_abi::stat::{
    S_IFBLK, S_IFCHR, S_IFDIR, S_IFIFO, S_IFLNK, S_IFMT, S_IFREG, S_IFSOCK,
};
use raven_libc_abi::stat::{
    S_IRGRP, S_IROTH, S_IRUSR, S_ISGID, S_ISUID, S_ISVTX, S_IWGRP, S_IWOTH, S_IWUSR, S_IXGRP,
    S_IXOTH, S_IXUSR,
};

/// Fill `out` with the ten `ls -l` mode characters, a space and a NUL (12 bytes).
pub fn strmode(mode: u32, out: &mut [u8; 12]) {
    let mut index = 0usize;
    let kind = mode & S_IFMT;
    let first = if kind == S_IFDIR {
        b'd'
    } else if kind == S_IFCHR {
        b'c'
    } else if kind == S_IFBLK {
        b'b'
    } else if kind == S_IFREG {
        b'-'
    } else if kind == S_IFLNK {
        b'l'
    } else if kind == S_IFSOCK {
        b's'
    } else if kind == S_IFIFO {
        b'p'
    } else {
        b'?'
    };
    if let Some(slot) = out.get_mut(index) {
        *slot = first;
    }
    index = index.wrapping_add(1);

    for (read, write, execute, special) in [
        (S_IRUSR, S_IWUSR, S_IXUSR, S_ISUID),
        (S_IRGRP, S_IWGRP, S_IXGRP, S_ISGID),
        (S_IROTH, S_IWOTH, S_IXOTH, S_ISVTX),
    ] {
        if let Some(slot) = out.get_mut(index) {
            *slot = if mode & read != 0 { b'r' } else { b'-' };
        }
        index = index.wrapping_add(1);
        if let Some(slot) = out.get_mut(index) {
            *slot = if mode & write != 0 { b'w' } else { b'-' };
        }
        index = index.wrapping_add(1);
        let execute_bit = mode & execute != 0;
        let special_bit = mode & special != 0;
        let byte = match (execute_bit, special_bit) {
            (false, false) => b'-',
            (true, false) => b'x',
            (false, true) => {
                if special == S_ISVTX {
                    b'T'
                } else {
                    b'S'
                }
            }
            (true, true) => {
                if special == S_ISVTX {
                    b't'
                } else {
                    b's'
                }
            }
        };
        if let Some(slot) = out.get_mut(index) {
            *slot = byte;
        }
        index = index.wrapping_add(1);
    }

    if let Some(slot) = out.get_mut(index) {
        *slot = b' ';
    }
    index = index.wrapping_add(1);
    if let Some(slot) = out.get_mut(index) {
        *slot = 0;
    }
}

#[cfg(test)]
#[path = "strmode_ut.rs"]
mod tests;
