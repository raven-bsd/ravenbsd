// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
// Copyright (c) 1980, 1993 The Regents of the University of California.  All rights reserved.
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/getbsize.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: getbsize.c,v 1.14 2015/10/09 01:37:07 millert Exp $)
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

//! `getbsize` (donor-map 15 T3, 02 X49).
//!
//! `BLOCKSIZE` is a parameter instead of `getenv`. The donor's `warnx` calls are dropped: the
//! crate has no output channel, the shell prints the fixed text through `err.h` when it wants
//! to. The header is `"%ld%s-blocks"` and is truncated to the donor's 20-byte static buffer.

#![allow(clippy::indexing_slicing, clippy::arithmetic_side_effects)] // direct transliteration: the indices are bounded by the donor algorithm
use alloc::vec::Vec;

const KB: i64 = 1024;
const MB: i64 = 1024 * 1024;
const GB: i64 = 1024 * 1024 * 1024;
const MAXB: i64 = GB;
const HEADER_CAP: usize = 20;

/// Parse the donor's `strtol(p, &ep, 10)` prefix: optional whitespace, sign and digits.
///
/// Returns `(value, remaining)` where `remaining` starts at `ep`.
fn strtol_decimal(input: &[u8]) -> (i64, &[u8]) {
    let mut index = 0usize;
    while index < input.len() && input[index].is_ascii_whitespace() {
        index = index.wrapping_add(1);
    }
    let mut negative = false;
    if index < input.len() && (input[index] == b'-' || input[index] == b'+') {
        negative = input[index] == b'-';
        index = index.wrapping_add(1);
    }
    let mut value: i64 = 0;
    let mut overflow = false;
    let start = index;
    while index < input.len() && input[index].is_ascii_digit() {
        let digit = i64::from(input[index].wrapping_sub(b'0'));
        value = match value
            .checked_mul(10)
            .and_then(|current| current.checked_add(digit))
        {
            Some(next) => next,
            None => {
                overflow = true;
                value
            }
        };
        index = index.wrapping_add(1);
    }
    if index == start {
        return (0, input.get(index..).unwrap_or(b""));
    }
    if negative {
        value = value.wrapping_neg();
    }
    if overflow {
        // strtol saturates at LONG_MAX/LONG_MIN; the donor only looks at the sign and the
        // suffix, and either way the clamp below turns it into the maximum.
        value = if negative { i64::MIN } else { i64::MAX };
    }
    (value, input.get(index..).unwrap_or(b""))
}

fn push_i64(out: &mut Vec<u8>, value: i64) {
    let mut buf = [0u8; 24];
    let mut index = buf.len();
    let mut magnitude = value.unsigned_abs();
    loop {
        index = index.wrapping_sub(1);
        buf[index] = b'0'.wrapping_add(u8::try_from(magnitude % 10).unwrap_or(0));
        magnitude /= 10;
        if magnitude == 0 {
            break;
        }
    }
    out.extend_from_slice(&buf[index..]);
}

/// `getbsize`.
///
/// Fills `headerlen` with the length of the returned header (capped to 19, like the donor's
/// 20-byte buffer) and `blocksize` with the block size. The header has no trailing NUL; the
/// shell copies it into the static buffer the donor returns.
#[must_use]
pub fn getbsize(blocksize_env: Option<&[u8]>, headerlen: &mut i32, blocksize: &mut i64) -> Vec<u8> {
    let mut n: i64;
    let max: i64;
    let mul: i64;
    let mut size: i64;
    let mut form: &[u8] = b"";

    match blocksize_env {
        Some(value) if !value.is_empty() && value.first().copied() != Some(0) => {
            let (parsed, rest) = strtol_decimal(value);
            n = parsed;
            if n < 0 {
                max = MAXB;
                mul = 1;
            } else {
                if n == 0 {
                    n = 1;
                }
                if rest.len() > 1 {
                    max = MAXB;
                    mul = 1;
                    n = 512;
                } else {
                    match rest.first().copied().unwrap_or(0) {
                        b'G' | b'g' => {
                            form = b"G";
                            max = 1;
                            mul = GB;
                        }
                        b'K' | b'k' => {
                            form = b"K";
                            max = 1_048_576;
                            mul = KB;
                        }
                        b'M' | b'm' => {
                            form = b"M";
                            max = 1_024;
                            mul = MB;
                        }
                        0 => {
                            max = MAXB;
                            mul = 1;
                        }
                        _ => {
                            n = 512;
                            max = MAXB;
                            mul = 1;
                        }
                    }
                }
            }
            if n > max {
                n = max;
            }
            size = n.wrapping_mul(mul);
            if size < 512 {
                form = b"";
                size = 512;
                n = 512;
            }
        }
        _ => {
            size = 512;
            n = 512;
        }
    }
    let mut header = Vec::new();
    push_i64(&mut header, n);
    header.extend_from_slice(form);
    header.extend_from_slice(b"-blocks");
    *blocksize = size;
    let length = if header.len() >= HEADER_CAP {
        HEADER_CAP.wrapping_sub(1)
    } else {
        header.len()
    };
    *headerlen = i32::try_from(length).unwrap_or(0);
    header.truncate(length);
    header
}

#[cfg(test)]
#[path = "getbsize_ut.rs"]
mod tests;
