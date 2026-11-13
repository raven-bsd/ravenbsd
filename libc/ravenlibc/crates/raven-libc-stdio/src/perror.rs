// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/perror.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// The donor file has no per-file header; it is covered by musl/COPYRIGHT (lines 1-24).
// MIT via musl/COPYRIGHT.
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
//
//! `perror` (`musl:src/stdio/perror.c`, R48).
//!
//! The message text comes from the caller (C40's `errmsg`); this function builds
//! `"<msg>: <text>\n"` (or `"<text>\n"` for an empty message), forces the byte orientation
//! for the duration of the call without changing the stream's orientation afterwards, and
//! reaches fd 2 with **one** `write2` call so concurrent writers cannot interleave inside the
//! message. `errno` is read once by the shell and passed in.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use alloc::vec::Vec;

use raven_libc_abi::generated::errno::EBADF;

use crate::file::File;
use crate::lock::FileGuard;
use crate::state::{State, StdKind};
use crate::traits::Env;
use crate::{Fail, R};

/// Write the current error message to `stderr`.
///
/// `message` is the caller's `const char *` (possibly empty); `text` is the `strerror` text
/// for the errno value the shell captured at entry.
///
/// # Errors
/// [`Fail::Errno`] with `EBADF` when there is no `stderr`, or the write error.
pub fn perror<E: Env>(state: &'static State<E>, message: &[u8], text: &[u8]) -> R<()> {
    let Some(file): Option<&'static File<E>> = state.std_file(StdKind::Err) else {
        return Err(Fail::from_int(EBADF));
    };
    let _guard = FileGuard::new(file);
    let saved_mode = file.st().mode;
    crate::mode::force_byte_orientation(file);
    let mut out = Vec::with_capacity(message.len() + text.len() + 4);
    if !message.is_empty() {
        out.extend_from_slice(message);
        out.extend_from_slice(b": ");
    }
    out.extend_from_slice(text);
    out.push(b'\n');
    let result = file.env().write2(file.st().fd, &[], &out);
    file.st_mut().mode = saved_mode;
    match result {
        Ok(count) if count == out.len() => Ok(()),
        Ok(_) => Err(Fail::Eof),
        Err(error) => Err(Fail::Errno(error)),
    }
}

#[cfg(test)]
#[path = "perror_ut.rs"]
mod tests;
