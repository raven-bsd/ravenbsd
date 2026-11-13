// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/__stdio_read.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! The descriptor backend: `__stdio_read`, `__stdio_write`, `__stdio_seek`, `__stdio_close`.
//!
//! `__stdio_write` is one gathering write of the pending FILE buffer and the caller's data,
//! retried on a short count exactly as the donor does; on error it drops the buffered data
//! (`wpos = wbase = wend = 0`) and sets `F_ERR`, and it never retries `EINTR`.
//!
//! `__stdio_read` uses two `read` calls where the donor uses one `readv`: the [`crate::SysFile`]
//! seam has no `readv` method (`donor-map/04` 5.1 row 2 allows this and asks for the note; the
//! report states it). The visible behaviour — bytes delivered to the caller, bytes parked in
//! the buffer, `F_EOF`/`F_ERR`, the file offset — is the donor's.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The donor walks `wpos`/`wbase` as pointers; the indices are the same arithmetic and every
// slice is bounded by the region length checked in `file.rs::check_invariants`.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::EIO;

use crate::file::{FileState, UNGET};
use crate::traits::Env;

/// The descriptor read backend.
pub(crate) fn read<E: Env>(st: &mut FileState<E>, env: &E, buf: &mut [u8]) -> Result<usize, Errno> {
    let len = buf.len();
    if len == 0 {
        return Ok(0);
    }
    let buffered = st.buf_size != 0;
    let caller_len = if buffered { len - 1 } else { len };
    if caller_len != 0 {
        let count = match env.read(st.fd, &mut buf[..caller_len]) {
            Ok(count) => count,
            Err(error) => {
                st.flags |= crate::file::F_ERR;
                return Err(error);
            }
        };
        if count < caller_len || !buffered {
            if count == 0 {
                st.flags |= crate::file::F_EOF;
            }
            return Ok(count);
        }
        // The caller part filled; the donor's single readv now fills the FILE buffer.
        let base = UNGET;
        let end = UNGET + st.buf_size;
        let region = st.buffer.bytes_mut();
        let count = match env.read(st.fd, &mut region[base..end]) {
            Ok(count) => count,
            Err(error) => {
                st.flags |= crate::file::F_ERR;
                return Err(error);
            }
        };
        if count == 0 {
            return Ok(caller_len);
        }
        st.rpos = base;
        st.rend = base + count;
        let first = region[st.rpos];
        st.rpos += 1;
        buf[len - 1] = first;
        return Ok(len);
    }

    // `len == 1` with a buffer: the donor reads straight into the buffer.
    let base = UNGET;
    let end = UNGET + st.buf_size;
    let region = st.buffer.bytes_mut();
    let count = match env.read(st.fd, &mut region[base..end]) {
        Ok(count) => count,
        Err(error) => {
            st.flags |= crate::file::F_ERR;
            return Err(error);
        }
    };
    if count == 0 {
        st.flags |= crate::file::F_EOF;
        return Ok(0);
    }
    st.rpos = base;
    st.rend = base + count;
    buf[0] = region[st.rpos];
    st.rpos += 1;
    Ok(1)
}

/// The descriptor write backend.
///
/// Returns the number of bytes of `buf` (the caller's data) accepted; the pending FILE
/// buffer is written first.
pub(crate) fn write<E: Env>(st: &mut FileState<E>, env: &E, buf: &[u8]) -> Result<usize, Errno> {
    let len = buf.len();
    let base = st.wbase;
    let pending = st.wpos.saturating_sub(base);
    let region = st.buffer.bytes_mut();
    let mut first: &[u8] = if pending == 0 {
        &[]
    } else {
        &region[base..st.wpos]
    };
    let mut second: &[u8] = buf;
    loop {
        let total = first.len().saturating_add(second.len());
        let count = match env.write2(st.fd, first, second) {
            Ok(count) => count,
            Err(error) => {
                st.wpos = 0;
                st.wbase = 0;
                st.wend = 0;
                st.flags |= crate::file::F_ERR;
                return Err(error);
            }
        };
        if count == total {
            st.wend = UNGET + st.buf_size;
            st.wpos = UNGET;
            st.wbase = UNGET;
            return Ok(len);
        }
        if count == 0 {
            st.wpos = 0;
            st.wbase = 0;
            st.wend = 0;
            st.flags |= crate::file::F_ERR;
            return Err(Errno(EIO as u16));
        }
        if count > first.len() {
            let used = count - first.len();
            if used >= second.len() {
                // The caller's data is complete; only the FILE buffer remains.
                st.wpos = base;
                st.wbase = base;
                st.wend = UNGET + st.buf_size;
                return Ok(len);
            }
            second = &second[used..];
            first = &[];
        } else {
            first = &first[count..];
        }
    }
}
