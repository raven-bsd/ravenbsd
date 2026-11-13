// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/__toread.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! The two mode switches (`musl:src/stdio/__toread.c`, `__towrite.c`).
//!
//! `towrite` drops the read window and activates the write window; `toread` flushes pending
//! output first, drops the write window, and arms the read window. Both fix the orientation
//! to byte with `mode |= mode - 1` when it is still unset, and both run the lazy buffer hook
//! (`F_LAZYBUF`) and the one-shot exit-cleanup registration first.
//!
//! `require_byte` is the public-entry rejection of a byte operation on a wide stream
//! (`donor-map/04` 7.2, T16); the switches themselves do not test the orientation, because
//! wide stdio goes through them too.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use crate::buf;
use crate::file::{F_EOF, F_ERR, F_LAZYBUF, F_NORD, F_NOWR};
use crate::traits::Env;
use crate::{Fail, File};
use raven_libc_abi::generated::errno::EBADF;

/// The `__toread` switch.
///
/// `Ok(true)` means the stream is at its end marker (the donor returns `EOF` without reading);
/// `Ok(false)` means a read may proceed. A read-only stream sets `F_ERR` and returns `EBADF`.
pub(crate) fn toread<E: Env>(file: &File<E>) -> Result<bool, Fail> {
    let _ = file.state().mark_used();
    if file.st().flags & F_LAZYBUF != 0 {
        buf::smakebuf_locked(file);
    }
    file.st_mut().orient_byte();
    if file.st().has_pending_write() {
        let _ = crate::backend::write(file, &[]);
    }
    let st = file.st_mut();
    st.clear_write();
    if st.flags & F_NORD != 0 {
        st.flags |= F_ERR;
        return Err(Fail::from_int(EBADF));
    }
    st.rpos = crate::file::UNGET + st.buf_size;
    st.rend = crate::file::UNGET + st.buf_size;
    Ok(st.flags & F_EOF != 0)
}

/// The `__towrite` switch.
pub(crate) fn towrite<E: Env>(file: &File<E>) -> Result<(), Fail> {
    let _ = file.state().mark_used();
    if file.st().flags & F_LAZYBUF != 0 {
        buf::smakebuf_locked(file);
    }
    let st = file.st_mut();
    st.orient_byte();
    if st.flags & F_NOWR != 0 {
        st.flags |= F_ERR;
        return Err(Fail::from_int(EBADF));
    }
    st.clear_read();
    st.wbase = crate::file::UNGET;
    st.wpos = crate::file::UNGET;
    st.wend = crate::file::UNGET + st.buf_size;
    Ok(())
}

/// Reject a byte operation on a wide-oriented stream (`donor-map/04` 7.2, T16).
///
/// This check lives at the public byte entry points, not in [`toread`]/[`towrite`], because
/// wide stdio itself goes through those switches.
///
/// # Errors
/// [`Fail::Errno`] with `EBADF`; `F_ERR` is set.
pub(crate) fn require_byte<E: Env>(file: &File<E>) -> Result<(), Fail> {
    if file.st().mode == 1 {
        file.st_mut().flags |= F_ERR;
        return Err(Fail::Errno(raven_libc_abi::errno::Errno(EBADF as u16)));
    }
    Ok(())
}

/// Force the byte orientation without touching the buffers (`perror`).
pub(crate) fn force_byte_orientation<E: Env>(file: &File<E>) {
    file.st_mut().orient_byte();
}

#[cfg(test)]
#[path = "mode_ut.rs"]
mod tests;
