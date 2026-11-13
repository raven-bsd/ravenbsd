// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/fseek.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! The seek family (`fseek.c`, `ftell.c`, `fgetpos.c`, `fsetpos.c`, `rewind.c`).
//!
//! `fseeko` validates `whence`, subtracts the unread window (including pushed-back bytes) for
//! `SEEK_CUR`, flushes the write buffer, drops the read window and clears `F_EOF`. `ftello`
//! adds the pending write bytes or subtracts the unread window and handles `F_APP` by asking
//! for `SEEK_END` first. On LP64 `fseek` and `fseeko` are the same function.
//!
//! `fpos_t` follows the O12 layout `{ off: i64, _pad: [u8; 16] }`. The headers area (C36)
//! has not published the type yet, so the crate defines it here with that exact layout; when
//! C36 lands, this definition becomes a re-export (report RD-new line).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

// The donor adds and subtracts pointer differences; the indices are the same values and every
// check the donor performs is kept.

use raven_libc_abi::generated::errno::{EINVAL, EIO, EOVERFLOW};
use raven_libc_abi::generated::fcntl::{SEEK_CUR, SEEK_END, SEEK_SET};

use crate::file::{F_APP, F_EOF, File};
use crate::lock::FileGuard;
use crate::traits::Env;
use crate::{Fail, R};

/// The C `fpos_t` (O12): an offset and room for a future multibyte state.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub struct Fpos {
    /// The byte offset.
    pub off: i64,
    /// Reserved for a future `mbstate_t`; currently zero.
    pub _pad: [u8; 16],
}

impl Fpos {
    /// A position from an offset.
    #[must_use]
    pub const fn new(off: i64) -> Self {
        Self { off, _pad: [0; 16] }
    }
}

/// The unlocked `fseeko` body (`__fseeko_unlocked`).
pub(crate) fn fseeko_unlocked<E: Env>(file: &File<E>, mut off: i64, whence: i32) -> R<()> {
    if whence != SEEK_CUR && whence != SEEK_SET && whence != SEEK_END {
        return Err(Fail::from_int(EINVAL));
    }
    if whence == SEEK_CUR && file.st().rend != 0 {
        let unread = file.st().unread();
        let Some(unread) = i64::try_from(unread).ok() else {
            return Err(Fail::from_int(EOVERFLOW));
        };
        let Some(adjusted) = off.checked_sub(unread) else {
            return Err(Fail::from_int(EOVERFLOW));
        };
        off = adjusted;
    }
    if file.st().has_pending_write() {
        if let Err(error) = crate::backend::write(file, &[]) {
            return Err(Fail::Errno(error));
        }
        if file.st().has_pending_write() {
            return Err(Fail::from_int(EIO));
        }
    }
    {
        let st = file.st_mut();
        st.wbase = 0;
        st.wpos = 0;
        st.wend = 0;
    }
    if let Err(error) = crate::backend::seek(file, off, whence) {
        return Err(Fail::Errno(error));
    }
    let st = file.st_mut();
    st.rpos = 0;
    st.rend = 0;
    st.flags &= !F_EOF;
    Ok(())
}

/// `fseeko`.
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL` for a bad `whence` or a negative resulting offset,
/// `EOVERFLOW` for an offset underflow, `ESPIPE` from an unseekable backend.
pub fn fseeko<E: Env>(file: &File<E>, off: i64, whence: i32) -> R<()> {
    let _guard = FileGuard::new(file);
    fseeko_unlocked(file, off, whence)
}

/// `fseek` (`long` is 64-bit on every target).
pub fn fseek<E: Env>(file: &File<E>, off: i64, whence: i32) -> R<()> {
    fseeko(file, off, whence)
}

/// The unlocked `ftello` body (`__ftello_unlocked`).
pub(crate) fn ftello_unlocked<E: Env>(file: &File<E>) -> R<i64> {
    let whence = if file.st().flags & F_APP != 0 && file.st().has_pending_write() {
        SEEK_END
    } else {
        SEEK_CUR
    };
    let base = match crate::backend::seek(file, 0, whence) {
        Ok(position) => position,
        Err(error) => return Err(Fail::Errno(error)),
    };
    if base < 0 {
        return Err(Fail::from_int(EIO));
    }
    if file.st().rend != 0 {
        let unread = i64::try_from(file.st().unread()).unwrap_or(0);
        return Ok(base.saturating_sub(unread));
    }
    if file.st().wbase != 0 {
        let pending = file.st().wpos.saturating_sub(file.st().wbase);
        let Some(pending) = i64::try_from(pending).ok() else {
            return Err(Fail::from_int(EOVERFLOW));
        };
        let Some(position) = base.checked_add(pending) else {
            return Err(Fail::from_int(EOVERFLOW));
        };
        return Ok(position);
    }
    Ok(base)
}

/// `ftello`.
///
/// # Errors
/// [`Fail::Errno`] with `ESPIPE` for a non-seekable backend and `EOVERFLOW`.
pub fn ftello<E: Env>(file: &File<E>) -> R<i64> {
    let _guard = FileGuard::new(file);
    ftello_unlocked(file)
}

/// `ftell`.
pub fn ftell<E: Env>(file: &File<E>) -> R<i64> {
    ftello(file)
}

/// `fgetpos`.
///
/// # Errors
/// The `ftello` errors.
pub fn fgetpos<E: Env>(file: &File<E>) -> R<Fpos> {
    Ok(Fpos::new(ftello(file)?))
}

/// `fsetpos`.
///
/// # Errors
/// The `fseeko` errors.
pub fn fsetpos<E: Env>(file: &File<E>, pos: &Fpos) -> R<()> {
    fseeko(file, pos.off, SEEK_SET)
}

/// `rewind`: seek to zero and clear both indicators.
pub fn rewind<E: Env>(file: &File<E>) {
    let _guard = FileGuard::new(file);
    let _ = fseeko_unlocked(file, 0, SEEK_SET);
    file.st_mut().flags &= !crate::file::F_ERR;
}

#[cfg(test)]
#[path = "seek_ut.rs"]
mod tests;
