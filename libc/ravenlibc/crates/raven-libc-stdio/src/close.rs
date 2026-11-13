// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/fclose.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! `fclose`, `__fclose_ca` and `fcloseall` (`fclose.c`, `__fclose_ca.c`; the list walk of
//! `freebsd:lib/libc/stdio/fcloseall.c`, BSD-2, rewritten over our open-file list).
//!
//! `fclose` flushes first, then closes the backend, then (unless `F_PERM`) unlinks the FILE
//! from the list while holding the list lock and frees it. A FILE with a `popen` child takes
//! the list lock before the close so a concurrent child scan cannot see a half-dead stream
//! (`popen.c:37-38`). The standard streams are never freed (R39).
//!
//! `fcloseall` collects the non-permanent FILEs first, then closes each; it never holds the
//! list lock across a `fclose`, and it leaves `stdin`/`stdout`/`stderr` open.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use crate::file::{F_PERM, File};
use crate::traits::Env;
use crate::{Fail, R};

/// `fclose`.
///
/// The FILE must come from [`crate::fopen`], [`crate::fdopen`] or one of the memory/cookie
/// constructors; standard streams are legal and are not freed.
///
/// # Errors
/// [`Fail::Errno`] with the flush or close error; the FILE is freed in every case.
pub fn fclose<E: Env>(file: &'static File<E>) -> R<()> {
    let mut error = None;
    if let Err(flush_error) = crate::flush::fflush(file.state(), Some(file)) {
        error = Some(flush_error);
    }
    let pipe = file.st().pipe_pid != 0;
    let guard = if pipe {
        Some(crate::ofl::OflGuard::acquire(file.state()))
    } else {
        None
    };
    if let Err(close_error) = crate::backend::close(file) {
        if error.is_none() {
            error = Some(Fail::Errno(close_error));
        }
    }
    let state = file.state();
    let permanent = file.st().flags & F_PERM != 0;
    if !permanent {
        if guard.is_some() {
            crate::ofl::remove_locked(state, file);
        } else {
            crate::ofl::remove(state, file);
        }
        {
            let st = file.st_mut();
            st.backend = crate::backend::Backend::Closed;
            st.buffer.clear();
            st.buf_size = 0;
            st.fd = -1;
            st.pipe_pid = 0;
            st.getln_buf = None;
            st.clear_read();
            st.clear_write();
        }
    }
    drop(guard);
    if !permanent {
        crate::raw::free_file((file as *const File<E>).cast_mut());
    }
    match error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

/// `__fclose_ca`: close a caller-allocated FILE without unlinking or freeing it.
///
/// # Errors
/// The backend close error.
pub fn fclose_ca<E: Env>(file: &File<E>) -> R<()> {
    match crate::backend::close(file) {
        Ok(()) => Ok(()),
        Err(error) => Err(Fail::Errno(error)),
    }
}

pub use crate::closeall::fcloseall;

#[cfg(test)]
#[path = "close_ut.rs"]
mod tests;
