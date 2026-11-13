// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1992, 1993 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/assert.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: assert.c,v 1.11 2015/09/13 08:31:47 guenther Exp $)
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

//! The `assert` text builder.
//!
//! H-13 supersedes the donor's `assertion "<expr>" failed: file ...` wording: the text is
//! `"<prog>: <file>:<line>: <func>: Assertion '<expr>' failed.\n"`. The three BSD entry points
//! share one builder; `__assert` has no function name and prints `?` there (RD-new 4). No
//! `fprintf`, no `malloc`, no locale: the shell `write(2)`s the buffer and calls `abort()`.

use raven_libc_str::itoa::write_i64;

/// The maximum message the shell writes (`[u8; 512]`).
pub const MESSAGE_CAPACITY: usize = 512;

fn push(out: &mut [u8; MESSAGE_CAPACITY], written: &mut usize, bytes: &[u8]) {
    for byte in bytes {
        if *written >= MESSAGE_CAPACITY.wrapping_sub(2) {
            return;
        }
        if let Some(slot) = out.get_mut(*written) {
            *slot = *byte;
            *written = written.wrapping_add(1);
        }
    }
}

/// Build the H-13 message; returns the number of bytes written, newline included when there is
/// room (at most 511 bytes of text plus the newline).
pub fn assert_message(
    out: &mut [u8; MESSAGE_CAPACITY],
    prog: &[u8],
    file: &[u8],
    line: i32,
    func: &[u8],
    expr: &[u8],
) -> usize {
    let mut written = 0usize;
    let prog = if prog.is_empty() {
        b"?".as_slice()
    } else {
        prog
    };
    let func = if func.is_empty() {
        b"?".as_slice()
    } else {
        func
    };
    push(out, &mut written, prog);
    push(out, &mut written, b": ");
    push(out, &mut written, file);
    push(out, &mut written, b":");
    let mut digits = [0u8; 24];
    let count = write_i64(&mut digits, i64::from(line));
    push(out, &mut written, digits.get(..count).unwrap_or(&[]));
    push(out, &mut written, b": ");
    push(out, &mut written, func);
    push(out, &mut written, b": Assertion `");
    push(out, &mut written, expr);
    push(out, &mut written, b"' failed.");
    if written < MESSAGE_CAPACITY
        && let Some(slot) = out.get_mut(written)
    {
        *slot = b'\n';
        written = written.wrapping_add(1);
    }
    written
}

/// `__assert_fail(expr, file, line, func)`.
pub fn assert_fail(
    out: &mut [u8; MESSAGE_CAPACITY],
    prog: &[u8],
    expr: &[u8],
    file: &[u8],
    line: i32,
    func: &[u8],
) -> usize {
    assert_message(out, prog, file, line, func, expr)
}

/// `__assert2(file, line, func, expr)`.
pub fn assert2(
    out: &mut [u8; MESSAGE_CAPACITY],
    prog: &[u8],
    file: &[u8],
    line: i32,
    func: &[u8],
    expr: &[u8],
) -> usize {
    assert_message(out, prog, file, line, func, expr)
}

/// `__assert(file, line, expr)`: no function name, so the builder prints `?` (RD-new 4).
pub fn assert_bsd(
    out: &mut [u8; MESSAGE_CAPACITY],
    prog: &[u8],
    file: &[u8],
    line: i32,
    expr: &[u8],
) -> usize {
    assert_message(out, prog, file, line, b"?", expr)
}

#[cfg(test)]
#[path = "assert_ut.rs"]
mod tests;
