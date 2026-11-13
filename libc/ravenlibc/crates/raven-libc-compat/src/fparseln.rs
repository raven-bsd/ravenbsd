// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1997 Christos Zoulas.  All rights reserved.
//
// RavenBSD: transpiled from freebsd:lib/libutil/fparseln.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
// (file revision tag: $NetBSD: fparseln.c,v 1.10 2009/10/21 01:07:45 snj Exp $)
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR ``AS IS'' AND ANY EXPRESS OR
// IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES
// OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE DISCLAIMED.
// IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY DIRECT, INDIRECT,
// INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT
// NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES; LOSS OF USE,
// DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY
// THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT
// (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE OF
// THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
//
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `fparseln` over [`LineSource`] (donor-map 15 T3, 02 X49).
//!
//! The donor reads through `fgetln`; the [`LineSource`] seam returns the same line (with its
//! newline, without a NUL). Continuations end in the continuation character, comments start at
//! the comment character unless escaped, and `lineno` counts every physical line read. The line
//! buffer cap is [`LINE_CAP`] (64 KiB): a longer logical line yields `None` and the caller sets
//! `ENOMEM`/`ERANGE` (the donor's `realloc` failure path).

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
use alloc::vec::Vec;

use crate::seams::LineSource;

/// `FPARSELN_UNESCESC`: remove `\<esc>` escapes.
pub const FPARSELN_UNESCESC: u32 = 0x01;
/// `FPARSELN_UNESCCONT`: remove `\<continuation>` escapes.
pub const FPARSELN_UNESCCONT: u32 = 0x02;
/// `FPARSELN_UNESCCOMM`: remove `\<comment>` escapes.
pub const FPARSELN_UNESCCOMM: u32 = 0x04;
/// `FPARSELN_UNESCREST`: remove escapes of every other character.
pub const FPARSELN_UNESCREST: u32 = 0x08;
/// `FPARSELN_UNESCALL`.
pub const FPARSELN_UNESCALL: u32 = 0x0f;

/// The cap of one logical line (ticket 4.1): 64 KiB.
pub const LINE_CAP: usize = 64 * 1024;

fn isescaped(sp: &[u8], position: usize, esc: u8) -> bool {
    if esc == 0 {
        return false;
    }
    let mut count = 0usize;
    let mut cursor = position;
    while cursor > 0 {
        cursor = cursor.wrapping_sub(1);
        if sp.get(cursor) == Some(&esc) {
            count = count.wrapping_add(1);
        } else {
            break;
        }
    }
    count & 1 != 0
}

/// Read one logical line.
///
/// `delim` is `[escape, continuation, comment]` (the donor's `dstr` is `b"\\\\#"`). `None` is
/// end of file or a line over [`LINE_CAP`].
pub fn fparseln<L: LineSource>(
    src: &mut L,
    delim: [u8; 3],
    flags: u32,
    lineno: Option<&mut usize>,
) -> Option<Vec<u8>> {
    let esc = delim[0];
    let con = delim[1];
    let com = delim[2];
    let nl = b'\n';
    let mut buffer: Option<Vec<u8>> = None;
    let mut line_number = lineno;
    let mut count = 1;
    while count != 0 {
        count = 0;
        if let Some(number) = line_number.as_deref_mut() {
            *number = number.wrapping_add(1);
        }
        let Some(line) = src.next_line() else {
            break;
        };
        let mut end = line.len();
        if end > 0 && com != 0 {
            let mut cursor = 0usize;
            while cursor < end {
                if line.get(cursor) == Some(&com) && !isescaped(&line, cursor, esc) {
                    end = cursor;
                    count = u32::from(end == 0 && buffer.is_none());
                    break;
                }
                cursor = cursor.wrapping_add(1);
            }
        }
        if end > 0 && line.get(end.wrapping_sub(1)) == Some(&nl) {
            end = end.wrapping_sub(1);
        }
        if end > 0
            && con != 0
            && line.get(end.wrapping_sub(1)) == Some(&con)
            && !isescaped(&line, end.wrapping_sub(1), esc)
        {
            end = end.wrapping_sub(1);
            count = 1;
        }
        if end == 0 && (count != 0 || buffer.is_some()) {
            continue;
        }
        let value = buffer.get_or_insert_with(Vec::new);
        if value.len().wrapping_add(end) > LINE_CAP {
            return None;
        }
        value.extend_from_slice(&line[..end]);
    }

    if flags & FPARSELN_UNESCALL != 0
        && esc != 0
        && let Some(value) = buffer.as_mut().filter(|value| value.contains(&esc))
    {
        let mut out = Vec::with_capacity(value.len());
        let mut cursor = 0usize;
        while cursor < value.len() {
            while value.get(cursor).is_some_and(|&byte| byte != esc) {
                out.push(value[cursor]);
                cursor = cursor.wrapping_add(1);
            }
            if cursor >= value.len() || cursor.wrapping_add(1) >= value.len() {
                break;
            }
            let next = value[cursor.wrapping_add(1)];
            let skip = if next == com {
                flags & FPARSELN_UNESCCOMM != 0
            } else if next == con {
                flags & FPARSELN_UNESCCONT != 0
            } else if next == esc {
                flags & FPARSELN_UNESCESC != 0
            } else {
                flags & FPARSELN_UNESCREST != 0
            };
            if skip {
                cursor = cursor.wrapping_add(1);
            } else {
                out.push(value[cursor]);
                cursor = cursor.wrapping_add(1);
            }
            out.push(value[cursor]);
            cursor = cursor.wrapping_add(1);
        }
        *value = out;
    }
    buffer
}

#[cfg(test)]
#[path = "fparseln_ut.rs"]
mod tests;
