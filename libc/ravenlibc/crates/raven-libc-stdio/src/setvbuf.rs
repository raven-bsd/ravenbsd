// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/setvbuf.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! `setvbuf` and its BSD wrappers (`setvbuf.c`, `setbuf.c`, `setbuffer.c`, `setlinebuf.c`).
//!
//! The behaviour is undefined unless this is the first operation on the stream, so the donor
//! takes no lock; the crate follows. `_IONBF` sets `buf_size = 0`; `_IOLBF` sets
//! `lbf = '\n'`; a caller buffer shorter than [`crate::file::UNGET`] is ignored (the donor's
//! `size >= UNGET` check); `F_SVB` is set in every accepted case and blocks the lazy buffer
//! policy and the stdout probe.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use raven_libc_abi::generated::errno::EINVAL;

use crate::file::{Buffer, File, UNGET};
use crate::raw::RawBuf;
use crate::traits::Env;
use crate::{Fail, R};

/// `_IOFBF`: fully buffered.
pub const IOFBF: i32 = 0;
/// `_IOLBF`: line buffered.
pub const IOLBF: i32 = 1;
/// `_IONBF`: unbuffered.
pub const IONBF: i32 = 2;

/// `setvbuf`.
///
/// `buf` is the caller's region; the C contract is that it outlives the stream.
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL` for an unknown `type`.
pub fn setvbuf<E: Env>(
    file: &File<E>,
    buf: Option<(*mut u8, usize)>,
    kind: i32,
    size: usize,
) -> R<()> {
    let st = file.st_mut();
    st.lbf = -1;
    match kind {
        IONBF => {
            st.buf_size = 0;
            st.clear_read();
            st.clear_write();
        }
        IOLBF | IOFBF => {
            let mut used_caller = false;
            if let Some((pointer, length)) = buf {
                let usable = size.min(length);
                if usable >= UNGET {
                    st.buffer = Buffer::Fixed(RawBuf::from_parts(pointer, usable));
                    st.buf_size = usable - UNGET;
                    used_caller = true;
                }
            }
            if !used_caller && st.buf_size == 0 {
                // A lazy stream has no buffer yet; musl's `__fdopen` allocated one before
                // `setvbuf` could be called, so the Rust form allocates the default region
                // now. A caller buffer smaller than UNGET is ignored but the stream keeps a
                // normal buffer (this is what makes the oracle's buffering match).
                let size = crate::buf::buffer_size(file);
                let region = crate::buf::default_region(size);
                if let Some(region) = region {
                    st.buffer = Buffer::Heap(region);
                    st.buf_size = size;
                }
            }
            if kind == IOLBF && st.buf_size != 0 {
                st.lbf = i32::from(b'\n');
            }
        }
        _ => return Err(Fail::from_int(EINVAL)),
    }
    st.flags |= crate::file::F_SVB;
    st.flags &= !crate::file::F_LAZYBUF;
    st.clear_read();
    st.clear_write();
    Ok(())
}

/// `setbuf`: fully buffered with `BUFSIZ`, or unbuffered with a null buffer.
pub fn setbuf<E: Env>(file: &File<E>, buf: Option<(*mut u8, usize)>) {
    let kind = if buf.is_some() { IOFBF } else { IONBF };
    let _ = setvbuf(file, buf, kind, crate::buf::BUFSIZ);
}

/// `setbuffer`: fully buffered with an explicit size.
pub fn setbuffer<E: Env>(file: &File<E>, buf: Option<(*mut u8, usize)>, size: usize) {
    let kind = if buf.is_some() { IOFBF } else { IONBF };
    let _ = setvbuf(file, buf, kind, size);
}

/// `setlinebuf`: line buffered.
pub fn setlinebuf<E: Env>(file: &File<E>) {
    let _ = setvbuf(file, None, IOLBF, 0);
}

#[cfg(test)]
#[path = "setvbuf_ut.rs"]
mod tests;
