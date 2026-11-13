// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/ext.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! `<stdio_ext.h>` (`ext.c`, `ext2.c`, R53; T10).
//!
//! `__fsetlocking(QUERY/INTERNAL/BYCALLER)` returns the previous state; a bad type returns the
//! old state and changes nothing (T10 chooses musl's ignore over Bionic's abort). The query
//! functions read the FILE state under the lock. `__freadptr` hands out an offset/length pair
//! into the FILE's region because a pointer would escape the borrow; the shell adds the region
//! address. `__fpurge`/`fpurge` drop both windows without writing; `_flushlbf` flushes every
//! line-buffered writer.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use crate::file::{F_BYCALLER, F_NORD, F_NOWR, File};
use crate::lock::FileGuard;
use crate::state::{State, StdKind};
use crate::traits::Env;
use crate::{Fail, R};

/// `FSETLOCKING_QUERY`.
pub const FSETLOCKING_QUERY: i32 = 0;
/// `FSETLOCKING_INTERNAL`.
pub const FSETLOCKING_INTERNAL: i32 = 1;
/// `FSETLOCKING_BYCALLER`.
pub const FSETLOCKING_BYCALLER: i32 = 2;

/// `__fsetlocking`.
pub fn __fsetlocking<E: Env>(file: &File<E>, kind: i32) -> i32 {
    let _guard = FileGuard::new(file);
    let previous = if file.st().flags & F_BYCALLER != 0 {
        FSETLOCKING_BYCALLER
    } else {
        FSETLOCKING_INTERNAL
    };
    match kind {
        FSETLOCKING_QUERY => {}
        FSETLOCKING_INTERNAL => file.st_mut().flags &= !F_BYCALLER,
        FSETLOCKING_BYCALLER => file.st_mut().flags |= F_BYCALLER,
        _ => {}
    }
    previous
}

/// `__fwriting`.
#[must_use]
pub fn __fwriting<E: Env>(file: &File<E>) -> bool {
    let _guard = FileGuard::new(file);
    file.st().flags & F_NORD != 0 || file.st().wend != 0
}

/// `__freading`.
#[must_use]
pub fn __freading<E: Env>(file: &File<E>) -> bool {
    let _guard = FileGuard::new(file);
    file.st().flags & F_NOWR != 0 || file.st().rend != 0
}

/// `__freadable`.
#[must_use]
pub fn __freadable<E: Env>(file: &File<E>) -> bool {
    let _guard = FileGuard::new(file);
    file.st().flags & F_NORD == 0
}

/// `__fwritable`.
#[must_use]
pub fn __fwritable<E: Env>(file: &File<E>) -> bool {
    let _guard = FileGuard::new(file);
    file.st().flags & F_NOWR == 0
}

/// `__flbf`.
#[must_use]
pub fn __flbf<E: Env>(file: &File<E>) -> bool {
    let _guard = FileGuard::new(file);
    file.st().lbf >= 0
}

/// `__fbufsize`.
#[must_use]
pub fn __fbufsize<E: Env>(file: &File<E>) -> usize {
    let _guard = FileGuard::new(file);
    file.st().buf_size
}

/// `__fpending`.
#[must_use]
pub fn __fpending<E: Env>(file: &File<E>) -> usize {
    let _guard = FileGuard::new(file);
    if file.st().wend != 0 {
        file.st().wpos.saturating_sub(file.st().wbase)
    } else {
        0
    }
}

/// `__freadahead`.
#[must_use]
pub fn __freadahead<E: Env>(file: &File<E>) -> usize {
    let _guard = FileGuard::new(file);
    if file.st().rend != 0 {
        file.st().rend.saturating_sub(file.st().rpos)
    } else {
        0
    }
}

/// `__freadptr`: the offset and length of the readable window.
#[must_use]
pub fn __freadptr<E: Env>(file: &File<E>) -> Option<(usize, usize)> {
    let _guard = FileGuard::new(file);
    if file.st().rpos == file.st().rend {
        None
    } else {
        Some((
            file.st().rpos,
            file.st().rend.saturating_sub(file.st().rpos),
        ))
    }
}

/// `__freadptrinc`: consume `inc` buffered bytes.
pub fn __freadptrinc<E: Env>(file: &File<E>, inc: usize) {
    let _guard = FileGuard::new(file);
    let st = file.st_mut();
    st.rpos = st.rpos.saturating_add(inc).min(st.rend);
}

/// `__fseterr`.
pub fn __fseterr<E: Env>(file: &File<E>) {
    let _guard = FileGuard::new(file);
    file.st_mut().flags |= crate::file::F_ERR;
}

/// `__fpurge`/`fpurge`.
///
/// # Errors
/// Never; the `R` shape matches the shell's uniform mapping.
pub fn __fpurge<E: Env>(file: &File<E>) -> R<()> {
    crate::flush::fpurge(file)
}

/// `_flushlbf`: flush every line-buffered writer.
///
/// # Errors
/// The first flush error.
pub fn _flushlbf<E: Env>(state: &'static State<E>) -> R<()> {
    let mut first_error = None;
    for kind in [StdKind::Out, StdKind::Err, StdKind::In] {
        if let Some(file) = state.std_file(kind) {
            flush_if_line_buffered(file, &mut first_error);
        }
    }
    let guard = crate::ofl::OflGuard::acquire(state);
    let mut cursor = guard.head();
    while let Some(file) = cursor {
        cursor = file.st().next;
        flush_if_line_buffered(file, &mut first_error);
    }
    match first_error {
        Some(error) => Err(error),
        None => Ok(()),
    }
}

fn flush_if_line_buffered<E: Env>(file: &File<E>, first_error: &mut Option<Fail>) {
    if file.st().lbf < 0 || !file.st().has_pending_write() {
        return;
    }
    let _guard = FileGuard::new(file);
    if let Err(error) = crate::backend::write(file, &[]) {
        if first_error.is_none() {
            *first_error = Some(Fail::Errno(error));
        }
    }
}

#[cfg(test)]
#[path = "ext_ut.rs"]
mod tests;
