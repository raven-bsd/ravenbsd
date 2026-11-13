// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/fopencookie.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! `fopencookie` (`musl:src/stdio/fopencookie.c`).
//!
//! The GNU `cookie_io_functions_t` is `crate::raw::CookieIo`: `read`, `write`, `seek` with
//! an `off64_t *` and `close`. The typed thunks live in `raw`; the cookie itself is an
//! ordinary field of the backend enum, so no side allocation is needed (R52). The stream is
//! the usual FILE with a lazy buffer, `fd = -1`, and the access flags derived from the mode
//! string.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

// The cookie read/write shapes are the donor's; every buffer range is bounded by its slice.

use core::ffi::c_void;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EINVAL, ENOTSUP, ESPIPE};

use crate::file::{Buffer, F_EOF, F_ERR, F_LAZYBUF, F_NORD, F_NOWR, File, FileState, UNGET};
use crate::raw::{CookieIo, RawPtr};
use crate::state::State;
use crate::traits::Env;
use crate::{Fail, R};

/// A GNU cookie stream's backend state.
pub(crate) struct CookieState {
    cookie: RawPtr<c_void>,
    io: CookieIo,
}

impl CookieState {
    /// The backend read (`cookieread`).
    pub(crate) fn read<E: Env>(
        &mut self,
        st: &mut FileState<E>,
        buf: &mut [u8],
    ) -> Result<usize, Errno> {
        let mut ret: isize = -1;
        let len = buf.len();
        let mut readlen = 0usize;
        let mut remain = len;
        let first = len.saturating_sub(usize::from(st.buf_size != 0));
        if first != 0 {
            match self.io.call_read(self.cookie.get(), &mut buf[..first]) {
                Some(count) if count > 0 => {
                    readlen += count as usize;
                    remain = remain.saturating_sub(count as usize);
                }
                Some(count) => {
                    ret = count;
                    return self.read_bail(st, ret, readlen);
                }
                None => return self.read_bail(st, ret, readlen),
            }
        }
        if st.buf_size == 0 || remain > 1 {
            return Ok(readlen);
        }
        let base = UNGET;
        let end = UNGET + st.buf_size;
        let region = st.buffer.bytes_mut();
        match self.io.call_read(self.cookie.get(), &mut region[base..end]) {
            Some(count) if count > 0 => {
                st.rpos = base;
                st.rend = base + count as usize;
                if let Some(byte) = region.get(st.rpos).copied() {
                    if let Some(sink) = buf.get_mut(readlen) {
                        *sink = byte;
                    }
                    st.rpos += 1;
                    readlen += 1;
                }
                Ok(readlen)
            }
            Some(count) => {
                ret = count;
                self.read_bail(st, ret, readlen)
            }
            None => self.read_bail(st, ret, readlen),
        }
    }

    /// The donor's `bail:` label.
    fn read_bail<E: Env>(
        &self,
        st: &mut FileState<E>,
        ret: isize,
        readlen: usize,
    ) -> Result<usize, Errno> {
        st.flags |= if ret == 0 { F_EOF } else { F_ERR };
        st.rpos = UNGET;
        st.rend = UNGET;
        Ok(readlen)
    }

    /// The backend write (`cookiewrite`).
    pub(crate) fn write<E: Env>(
        &mut self,
        st: &mut FileState<E>,
        buf: &[u8],
    ) -> Result<usize, Errno> {
        let pending = st.wpos.saturating_sub(st.wbase);
        if pending != 0 {
            let start = st.wbase;
            st.wpos = st.wbase;
            let mut region = core::mem::replace(&mut st.buffer, Buffer::Empty);
            let ok = {
                let bytes = region.bytes_mut();
                match bytes.get(start..start + pending) {
                    Some(slice) => self.write_cookie(st, slice) == pending,
                    None => false,
                }
            };
            st.buffer = region;
            if !ok {
                return Ok(0);
            }
        }
        Ok(self.write_cookie(st, buf))
    }

    /// One cookie write without the pending FILE buffer.
    fn write_cookie<E: Env>(&mut self, st: &mut FileState<E>, buf: &[u8]) -> usize {
        let Some(count) = self.io.call_write(self.cookie.get(), buf) else {
            return buf.len();
        };
        if count < 0 {
            st.wpos = 0;
            st.wbase = 0;
            st.wend = 0;
            st.flags |= F_ERR;
            return 0;
        }
        count as usize
    }

    /// The backend seek (`cookieseek`).
    pub(crate) fn seek<E: Env>(
        &mut self,
        _st: &mut FileState<E>,
        off: i64,
        whence: i32,
    ) -> Result<i64, Errno> {
        if whence > 2 {
            return Err(Errno(EINVAL as u16));
        }
        match self.io.call_seek(self.cookie.get(), off, whence) {
            Some((result, offset)) if result >= 0 => Ok(offset),
            Some(_) => Err(Errno(ESPIPE as u16)),
            None => Err(Errno(ENOTSUP as u16)),
        }
    }

    /// The backend close (`cookieclose`).
    pub(crate) fn close<E: Env>(&mut self, _st: &mut FileState<E>) -> Result<(), Errno> {
        match self.io.call_close(self.cookie.get()) {
            Some(0) | None => Ok(()),
            Some(_) => Err(Errno(EINVAL as u16)),
        }
    }
}

/// `fopencookie`.
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL` for a bad mode; `ENOMEM` cannot occur (no side allocation),
/// but the FILE allocation itself is fallible through the global allocator.
pub fn fopencookie<E: Env>(
    state: &'static State<E>,
    cookie: *mut c_void,
    mode: &[u8],
    io: CookieIo,
) -> R<*mut File<E>> {
    let Some(&first) = mode.first() else {
        return Err(Fail::from_int(EINVAL));
    };
    if !matches!(first, b'r' | b'w' | b'a') {
        return Err(Fail::from_int(EINVAL));
    }
    let plus = mode.contains(&b'+');
    let mut flags = F_LAZYBUF;
    if !plus {
        flags |= if first == b'r' { F_NOWR } else { F_NORD };
    }
    let pointer = crate::raw::into_raw(alloc::boxed::Box::new(File::new(state)));
    let file: &'static File<E> = crate::raw::file_ref(pointer);
    let st = file.st_mut();
    st.fd = -1;
    st.flags = flags;
    st.lbf = -1;
    st.backend = crate::backend::Backend::Cookie(CookieState {
        cookie: RawPtr::new(cookie),
        io,
    });
    st.buffer = Buffer::Heap(alloc::vec![0u8; UNGET + crate::buf::BUFSIZ]);
    st.buf_size = crate::buf::BUFSIZ;
    st.clear_read();
    st.clear_write();
    crate::ofl::add(state, file);
    Ok(pointer)
}

#[cfg(test)]
#[path = "cookie_ut.rs"]
mod tests;
