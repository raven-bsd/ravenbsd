// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/ungetc.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! `ungetc` (`musl:src/stdio/ungetc.c`, `donor-map/04` A-stdio-5, R37).
//!
//! The pushback lives in the [`crate::file::UNGET`]-byte area *before* the buffer start, so
//! up to eight bytes are pushed without allocation. The pushed byte becomes part of the unread
//! window `rend - rpos`, so `ftell` moves back by one and `fseek(SEEK_CUR)` subtracts it and
//! then discards the buffer. `ungetc(EOF)` is a no-op returning `EOF`, and a write-only stream
//! fails **without** setting `F_ERR` (the area file's rule; musl's `__toread` would set it).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use crate::file::{F_EOF, F_NORD, File};
use crate::lock::FileGuard;
use crate::traits::Env;
use crate::{EOF, Fail, R};

/// `ungetc`: push one byte back.
///
/// # Errors
/// [`Fail::Eof`] for `EOF`, a write-only stream, or a full pushback area.
pub fn ungetc<E: Env>(file: &File<E>, c: i32) -> R<i32> {
    if c == EOF {
        return Err(Fail::Eof);
    }
    let _guard = FileGuard::new(file);
    if file.st().flags & F_NORD != 0 {
        // The donor's `__toread` flushes pending output before it rejects the read; our rule
        // keeps `F_ERR` clear, but the flush is observable in the write trace.
        if file.st().has_pending_write() {
            let _ = crate::backend::write(file, &[]);
        }
        return Err(Fail::Eof);
    }
    if file.st().rpos == 0 && file.st().rend == 0 {
        let _ = crate::mode::toread(file);
    }
    if file.st().rpos == 0 {
        return Err(Fail::Eof);
    }
    let rpos = file.st().rpos;
    let target = rpos - 1;
    {
        let st = file.st_mut();
        let region = st.buffer.bytes_mut();
        if let Some(slot) = region.get_mut(target) {
            *slot = c as u8;
        }
        st.rpos = target;
        st.flags &= !F_EOF;
    }
    Ok(i32::from(c as u8))
}

#[cfg(test)]
#[path = "ungetc_ut.rs"]
mod tests;
