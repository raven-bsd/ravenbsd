// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/signal/psignal.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// RavenBSD: transpiled from musl:src/signal/psiginfo.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// donor files have no per-file header; covered by musl/COPYRIGHT
//
// Permission is hereby granted, free of charge, to any person obtaining
// a copy of this software and associated documentation files (the
// "Software"), to deal in the Software without restriction, including
// without limitation the rights to use, copy, modify, merge, publish,
// distribute, sublicense, and/or sell copies of the Software, and to
// permit persons to whom the Software is furnished to do so, subject to
// the following conditions:
//
// The above copyright notice and this permission notice shall be
// included in all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY
// CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT,
// TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE
// SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

//! `psignal`/`psiginfo` text layout: `"<msg>: <text>\n"`.
//!
//! musl calls `strsignal` and `fprintf`; this crate is `no_std` and owns no stdio, so the text is
//! a parameter: [`psignal_text_with`] takes C40's `strsignal` text. [`psignal_text`] is the
//! ticket's fixed signature and falls back to the canonical name of [`crate::names`] (for example
//! `"HUP"`); the shell passes C40's text through the `_with` form. A `NULL` or empty message
//! produces only the text and the newline. The return value is the number of bytes written
//! (excluding the trailing NUL) and the output is always NUL-terminated when it has room.

use crate::names::name_of;
use raven_libc_abi::structs::SiginfoT;

/// `"<msg>: <text>\n"` with the canonical name as text.
pub fn psignal_text(sig: i32, msg: &[u8], out: &mut [u8]) -> usize {
    let text = name_of(sig).unwrap_or("Unknown signal");
    psignal_text_with(sig, msg, text.as_bytes(), out)
}

/// `"<msg>: <text>\n"` with the C40 text supplied by the caller.
pub fn psignal_text_with(_sig: i32, msg: &[u8], text: &[u8], out: &mut [u8]) -> usize {
    let mut cursor = 0usize;
    if !msg.is_empty() {
        cursor = cursor.saturating_add(append(out, cursor, msg));
        cursor = cursor.saturating_add(append(out, cursor, b": "));
    }
    cursor = cursor.saturating_add(append(out, cursor, text));
    cursor = cursor.saturating_add(append(out, cursor, b"\n"));
    if let Some(slot) = out.get_mut(cursor.min(out.len().saturating_sub(1))) {
        // The NUL lives at the end of the written prefix when there is room; if the buffer was
        // too small the last byte is overwritten, which is the documented truncation rule.
        *slot = 0;
    }
    cursor
}

/// `psiginfo`: the `si_signo` of the record and the same layout.
pub fn psiginfo_text(info: &SiginfoT, msg: &[u8], out: &mut [u8]) -> usize {
    psignal_text(info.si_signo, msg, out)
}

fn append(out: &mut [u8], at: usize, bytes: &[u8]) -> usize {
    let mut written = 0usize;
    for byte in bytes {
        let index = at.saturating_add(written);
        if let Some(slot) = out.get_mut(index) {
            *slot = *byte;
            written = written.saturating_add(1);
        } else {
            break;
        }
    }
    written
}

#[cfg(test)]
#[path = "psignal_ut.rs"]
mod psignal_ut;
