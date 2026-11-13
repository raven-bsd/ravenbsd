// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/remove.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! `remove`, `rename` and `tmpfile` (`remove.c`, `rename.c`, `tmpfile.c`).
//!
//! `remove` retries a directory with `rmdir` when `unlink` fails with `EISDIR`. `tmpfile`
//! tries `/tmp/tmpfile_XXXXXX` up to 100 times with `O_RDWR|O_CREAT|O_EXCL` (mode `0600`),
//! unlinks immediately and wraps the descriptor with `fdopen("w+")`; the directory is fixed
//! (`/tmp`, O13). `tmpnam` and `tempnam` are not provided (T7).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EEXIST, EISDIR};
use raven_libc_abi::generated::fcntl::{O_CREAT, O_EXCL, O_RDWR};

use crate::file::File;
use crate::open::fdopen;
use crate::state::State;
use crate::traits::Env;
use crate::{Fail, R};

/// The number of `tmpfile` attempts.
const MAXTRIES: usize = 100;

/// The `tmpfile` path template, without the six random characters.
const TMPFILE_PREFIX: &[u8] = b"/tmp/tmpfile_";

/// `remove`.
///
/// # Errors
/// The platform `unlink`/`rmdir` error.
pub fn remove<E: Env>(state: &'static State<E>, path: &[u8]) -> R<()> {
    match state.env().unlink(path) {
        Ok(()) => Ok(()),
        Err(error) if error.0 == EISDIR as u16 => match state.env().rmdir(path) {
            Ok(()) => Ok(()),
            Err(error) => Err(Fail::Errno(error)),
        },
        Err(error) => Err(Fail::Errno(error)),
    }
}

/// `rename`.
///
/// # Errors
/// The platform `rename` error.
pub fn rename<E: Env>(state: &'static State<E>, old: &[u8], new: &[u8]) -> R<()> {
    match state.env().rename(old, new) {
        Ok(()) => Ok(()),
        Err(error) => Err(Fail::Errno(error)),
    }
}

/// `tmpfile`: create an anonymous temporary stream.
///
/// # Errors
/// The last `open` error when every attempt failed, or the `fdopen` error.
pub fn tmpfile<E: Env>(state: &'static State<E>) -> R<*mut File<E>> {
    let mut last = None;
    for _ in 0..MAXTRIES {
        let mut suffix = [0u8; 6];
        state.env().random_suffix(&mut suffix);
        let mut path = alloc::vec::Vec::with_capacity(TMPFILE_PREFIX.len() + suffix.len());
        path.extend_from_slice(TMPFILE_PREFIX);
        path.extend_from_slice(&suffix);
        match state.env().open(&path, O_RDWR | O_CREAT | O_EXCL, 0o600) {
            Ok(fd) => {
                let _ = state.env().unlink(&path);
                return match fdopen(state, fd, b"w+") {
                    Ok(file) => Ok(file),
                    Err(error) => {
                        let _ = state.env().close(fd);
                        Err(error)
                    }
                };
            }
            Err(error) => last = Some(error),
        }
    }
    Err(Fail::Errno(last.unwrap_or(Errno(EEXIST as u16))))
}

#[cfg(test)]
#[path = "misc_ut.rs"]
mod tests;
