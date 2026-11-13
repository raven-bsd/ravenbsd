// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993
//	The Regents of the University of California.  All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strsep.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: strsep.c,v 1.8 2015/08/31 02:53:57 guenther Exp $)
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

//! `strsep` over a caller-owned buffer and cursor.

use super::byte_at;

/// Next field of the NUL-terminated buffer `s`, delimiters included as empty fields.
///
/// `pos` is the offset of the next field; `None` means there are no more fields. Returns
/// `(start, len)` and writes a NUL over the delimiter (the C `s[-1] = 0`).
pub fn strsep(s: &mut [u8], pos: &mut Option<usize>, delim: &[u8]) -> Option<(usize, usize)> {
    let start = (*pos)?;
    let mut index = start;
    loop {
        let byte = byte_at(s, index);
        if byte == 0 {
            *pos = None;
            return Some((start, index.wrapping_sub(start)));
        }
        index = index.wrapping_add(1);
        if crate::mem::find(delim, byte).is_some() {
            if let Some(slot) = s.get_mut(index.wrapping_sub(1)) {
                *slot = 0;
            }
            *pos = Some(index);
            return Some((start, index.wrapping_sub(1).wrapping_sub(start)));
        }
    }
}

#[cfg(test)]
#[path = "strsep_ut.rs"]
mod tests;
