// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/errno/strerror.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header; covered by musl/COPYRIGHT)
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

//! `strerror`: static text for known numbers, `"Unknown error: N"` for the rest.

use super::BUF_LEN;
use super::tables;
use crate::itoa::write_i64;

/// Prefix of the unknown-number text, without its trailing space.
pub(crate) const UNKNOWN_PREFIX: &[u8] = b"Unknown error:";

/// Static message for `e`, without a NUL, when `e` is a Linux `errno` in range.
///
/// RavenBSD: musl's `if (e >= len) e = 0` catch-all becomes `None` here; the caller decides
/// between the catch-all text and the `"Unknown error: N"` path (RD-13).
#[must_use]
pub fn message(e: i32) -> Option<&'static [u8]> {
    if e < 0 {
        return None;
    }
    tables::text(e as usize)
}

/// Write `"Unknown error: N"` into `buf` and return the number of bytes written (no NUL).
pub(crate) fn write_unknown(buf: &mut [u8], e: i32) -> usize {
    write_unknown_prefixed(buf, UNKNOWN_PREFIX, e)
}

/// Write `prefix`, a space and the decimal value into `buf`; returns the bytes written (no NUL).
pub(crate) fn write_unknown_prefixed(buf: &mut [u8], prefix: &[u8], value: i32) -> usize {
    let mut written = 0usize;
    for &byte in prefix {
        if let Some(slot) = buf.get_mut(written) {
            *slot = byte;
        } else {
            return written;
        }
        written = written.wrapping_add(1);
    }
    if let Some(slot) = buf.get_mut(written) {
        *slot = b' ';
    } else {
        return written;
    }
    written = written.wrapping_add(1);
    let digits = match buf.get_mut(written..) {
        Some(rest) => rest,
        None => return written,
    };
    written.wrapping_add(write_i64(digits, i64::from(value)))
}

/// Message for `e` into the caller's 64-byte buffer: `(text without NUL, known)`.
///
/// For an unknown number the buffer holds `"Unknown error: N"` and `known` is `false`; the shell
/// sets `errno = EINVAL` there, this engine never touches `errno`.
#[must_use]
pub fn strerror_into(e: i32, buf: &mut [u8; BUF_LEN]) -> (&[u8], bool) {
    if let Some(text) = message(e) {
        return (text, true);
    }
    let written = write_unknown(buf, e);
    (buf.get(..written).unwrap_or(&[]), false)
}

#[cfg(test)]
#[path = "strerror_ut.rs"]
mod tests;
