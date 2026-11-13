// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1988 Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/string/strtok.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (no $OpenBSD$ tag in the donor; the licence text is the UCB BSD-3 below)
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

//! `strtok`/`strtok_r` over a caller-owned buffer and cursor.
//!
//! RavenBSD: the donor's function-local `static char *last` is deleted; the cursor is the
//! `Option<usize>` parameter (RD-14). The cursor becomes `None` after the final token.

use super::byte_at;

/// Next token of the NUL-terminated buffer `s`.
///
/// `cursor` is the byte offset where scanning resumes; `None` means the string is exhausted. The
/// delimiter set excludes its NUL. Returns `(start, len)` of the token and writes a NUL over the
/// delimiter that ended it (the C `s[-1] = '\0'`).
pub fn strtok(s: &mut [u8], cursor: &mut Option<usize>, delim: &[u8]) -> Option<(usize, usize)> {
    // C: `last` is NULL after the final token; a new scan only starts when the caller passes a
    // string (raw::strtok_r resets the cursor to Some(0) there).
    let mut index = (*cursor)?;

    // Skip leading delimiters (C: cont: c = *s++; for (...)).
    loop {
        let byte = byte_at(s, index);
        if byte == 0 {
            *cursor = None;
            return None;
        }
        index = index.wrapping_add(1);
        if !crate::mem::find(delim, byte).is_some() {
            break;
        }
    }

    let start = index.wrapping_sub(1);
    loop {
        let byte = byte_at(s, index);
        index = index.wrapping_add(1);
        if byte == 0 {
            *cursor = None;
            return Some((start, index.wrapping_sub(1).wrapping_sub(start)));
        }
        if crate::mem::find(delim, byte).is_some() {
            if let Some(slot) = s.get_mut(index.wrapping_sub(1)) {
                *slot = 0;
            }
            *cursor = Some(index);
            return Some((start, index.wrapping_sub(1).wrapping_sub(start)));
        }
    }
}

#[cfg(test)]
#[path = "strtok_ut.rs"]
mod tests;
