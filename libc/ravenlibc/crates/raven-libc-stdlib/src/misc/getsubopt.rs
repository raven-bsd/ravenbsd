// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990, 1993 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/getsubopt.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: getsubopt.c,v 1.4 2005/08/08 08:05:36 espie Exp $)
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

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)] // RavenBSD: donor in-place offsets

//! `getsubopt`: split a comma/space/tab separated suboption list in place.

/// The result of one `getsubopt` call.

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct Subopt {
    /// The matched token index, or `None` for `-1`.
    pub index: Option<usize>,
    /// The token's `(start, len)` inside the buffer, or `None` at end of list.
    pub token: Option<(usize, usize)>,
    /// The value's `(start, len)` after `=`, or `None`.
    pub value: Option<(usize, usize)>,
    /// The offset the shell stores back into `*optionp` for the next call.
    pub next: usize,
}

/// Parse one token; writes NULs at the donor's delimiters.
///
/// `pos` is the shell's `*optionp` offset; `tokens` are the accepted names. The donor also sets
/// the global `suboptarg` to the token (the shell does that from [`Subopt::token`]).
pub fn getsubopt(pos: usize, options: &mut [u8], tokens: &[&[u8]]) -> Subopt {
    let mut p = pos;
    while let Some(&byte) = options.get(p) {
        if byte != b',' && byte != b' ' && byte != b'\t' {
            break;
        }
        p = p.wrapping_add(1);
    }
    let Some(&first) = options.get(p) else {
        return Subopt {
            index: None,
            token: None,
            value: None,
            next: p,
        };
    };
    if first == 0 {
        return Subopt {
            index: None,
            token: None,
            value: None,
            next: p,
        };
    }
    let token_start = p;
    // The donor starts the scan at the second character, so a leading `=` is token text.
    let mut scan = p.wrapping_add(1);
    while let Some(&byte) = options.get(scan) {
        if byte == 0 || byte == b',' || byte == b'=' || byte == b' ' || byte == b'\t' {
            break;
        }
        scan = scan.wrapping_add(1);
    }
    let token_end = scan;
    let mut value = None;
    match options.get(scan).copied() {
        Some(b'=') => {
            if let Some(slot) = options.get_mut(scan) {
                *slot = 0;
            }
            let value_start = scan.wrapping_add(1);
            let mut value_end = value_start;
            while let Some(&byte) = options.get(value_end) {
                if byte == 0 || byte == b',' || byte == b' ' || byte == b'\t' {
                    break;
                }
                value_end = value_end.wrapping_add(1);
            }
            value = Some((value_start, value_end.wrapping_sub(value_start)));
            p = value_end;
            if options.get(p).copied() == Some(b',')
                || options.get(p).copied() == Some(b' ')
                || options.get(p).copied() == Some(b'\t')
            {
                if let Some(slot) = options.get_mut(p) {
                    *slot = 0;
                }
                p = p.wrapping_add(1);
            }
        }
        Some(0) => {
            p = scan;
        }
        Some(_) => {
            if let Some(slot) = options.get_mut(scan) {
                *slot = 0;
            }
            p = scan.wrapping_add(1);
        }
        None => {
            p = scan;
        }
    }
    while let Some(&byte) = options.get(p) {
        if byte != b',' && byte != b' ' && byte != b'\t' {
            break;
        }
        p = p.wrapping_add(1);
    }
    let token_len = token_end.wrapping_sub(token_start);
    let token_slice = options.get(token_start..token_end).unwrap_or(&[]);
    let index = tokens.iter().position(|name| *name == token_slice);
    let _ = token_len;
    Subopt {
        index,
        token: Some((token_start, token_len)),
        value,
        next: p,
    }
}

#[cfg(test)]
#[path = "getsubopt_ut.rs"]
mod tests;
