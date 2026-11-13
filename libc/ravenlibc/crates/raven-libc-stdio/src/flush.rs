// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/fflush.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! `fflush`/`fpurge` (`musl:src/stdio/fflush.c`, A-stdio-7).
//!
//! `fflush(NULL)` visits `stdout`, then `stderr`, then the open-file list under the list
//! lock, taking one FILE lock at a time (trap S6). It flushes writers and repositions read
//! streams with `seek(rpos - rend, SEEK_CUR)` so the descriptor offset matches the logical
//! position (POSIX 2.5.1). Every stream is attempted; the first error is returned.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use raven_libc_abi::generated::fcntl::SEEK_CUR;

use crate::R;
use crate::file::File;
use crate::lock::FileGuard;
use crate::state::{State, StdKind};
use crate::traits::Env;

/// Flush the pending output and reposition a read stream; the caller holds the FILE lock.
fn flush_locked<E: Env>(file: &File<E>) -> R<()> {
    if file.st().has_pending_write() {
        if let Err(error) = crate::backend::write(file, &[]) {
            return Err(crate::Fail::Errno(error));
        }
        if file.st().has_pending_write() {
            return Err(crate::Fail::Errno(raven_libc_abi::errno::Errno(
                raven_libc_abi::generated::errno::EIO as u16,
            )));
        }
    }
    if file.st().rpos != file.st().rend {
        let unread = file.st().unread();
        let unread = i64::try_from(unread).unwrap_or(0);
        let _ = crate::backend::seek(file, -unread, SEEK_CUR);
    }
    let st = file.st_mut();
    st.clear_write();
    st.clear_read();
    Ok(())
}

/// `fflush`: flush one stream, or every stream when `file` is `None`.
///
/// # Errors
/// [`crate::Fail::Errno`] with the first flush error; every other stream is still flushed.
pub fn fflush<E: Env>(state: &'static State<E>, file: Option<&File<E>>) -> R<()> {
    match file {
        Some(file) => {
            let _guard = FileGuard::new(file);
            flush_locked(file)
        }
        None => fflush_all(state),
    }
}

/// `fflush(NULL)`: `stdout`, `stderr`, then the open-file list, one FILE lock at a time.
///
/// # Errors
/// The first flush error, after every stream was attempted.
pub fn fflush_all<E: Env>(state: &'static State<E>) -> R<()> {
    let mut first_error = None;
    for kind in [StdKind::Out, StdKind::Err] {
        if let Some(file) = state.std_file(kind) {
            let _guard = FileGuard::new(file);
            if let Err(error) = flush_locked(file) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    let guard = crate::ofl::OflGuard::acquire(state);
    let mut cursor = guard.head();
    while let Some(file) = cursor {
        let next = file.st().next;
        let _guard = FileGuard::new(file);
        if let Err(error) = flush_locked(file) {
            if first_error.is_none() {
                first_error = Some(error);
            }
        }
        cursor = next;
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// `fpurge`/`__fpurge`: drop both windows without writing (`ext.c`).
pub fn fpurge<E: Env>(file: &File<E>) -> R<()> {
    let _guard = FileGuard::new(file);
    let st = file.st_mut();
    st.clear_write();
    st.clear_read();
    Ok(())
}

#[cfg(test)]
#[path = "flush_ut.rs"]
mod tests;
