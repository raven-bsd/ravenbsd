// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! `funopen` (`donor-map/04` row 22, CLEANROOM over the cookie backend).
//!
//! The BSD contract: `readfn(cookie, buf, int n) -> int`, `writefn`,
//! `seekfn(cookie, fpos_t off, int whence) -> fpos_t`, `closefn(cookie) -> int`. A missing
//! `seekfn` makes the stream unseekable (`ESPIPE` on `fseek`); both a missing read and a
//! missing write function is `EINVAL`. The callbacks are stored in the same backend enum as
//! `fopencookie` and called through the typed thunks of `raw`; the source is
//! `freebsd:lib/libc/stdio/funopen.c` read for the contract only (never copied).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use core::ffi::{c_int, c_void};

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EINVAL, ESPIPE};

use crate::file::{Buffer, F_EOF, F_ERR, F_LAZYBUF, F_NORD, F_NOWR, File, FileState, UNGET};
use crate::raw::{FunIo, RawPtr};
use crate::state::State;
use crate::traits::Env;
use crate::{Fail, R};

/// A BSD `funopen` stream's backend state.
pub(crate) struct FunState {
    cookie: RawPtr<c_void>,
    io: FunIo,
}

impl FunState {
    /// The backend read.
    pub(crate) fn read<E: Env>(
        &mut self,
        st: &mut FileState<E>,
        buf: &mut [u8],
    ) -> Result<usize, Errno> {
        match self.io.call_read(self.cookie.get(), buf) {
            Some(count) if count >= 0 => {
                if count == 0 {
                    st.flags |= F_EOF;
                }
                Ok(count as usize)
            }
            Some(_) => {
                st.flags |= F_ERR;
                Ok(0)
            }
            None => {
                st.flags |= F_ERR;
                Ok(0)
            }
        }
    }

    /// The backend write.
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

    fn write_cookie<E: Env>(&mut self, st: &mut FileState<E>, buf: &[u8]) -> usize {
        match self.io.call_write(self.cookie.get(), buf) {
            Some(count) if count >= 0 => count as usize,
            Some(_) => {
                st.wpos = 0;
                st.wbase = 0;
                st.wend = 0;
                st.flags |= F_ERR;
                0
            }
            None => {
                st.flags |= F_ERR;
                0
            }
        }
    }

    /// The backend seek; a missing callback is `ESPIPE`.
    pub(crate) fn seek<E: Env>(
        &mut self,
        _st: &mut FileState<E>,
        off: i64,
        whence: i32,
    ) -> Result<i64, Errno> {
        match self.io.call_seek(self.cookie.get(), off, whence) {
            Some(position) if position >= 0 => Ok(position),
            Some(_) => Err(Errno(ESPIPE as u16)),
            None => Err(Errno(ESPIPE as u16)),
        }
    }

    /// The backend close.
    pub(crate) fn close<E: Env>(&mut self, _st: &mut FileState<E>) -> Result<(), Errno> {
        match self.io.call_close(self.cookie.get()) {
            Some(0) | None => Ok(()),
            Some(_) => Err(Errno(EINVAL as u16)),
        }
    }
}

/// `funopen`.
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL` when both `readfn` and `writefn` are missing.
pub fn funopen<E: Env>(
    state: &'static State<E>,
    cookie: *mut c_void,
    readfn: Option<unsafe extern "C" fn(*mut c_void, *mut core::ffi::c_char, c_int) -> c_int>,
    writefn: Option<unsafe extern "C" fn(*mut c_void, *const core::ffi::c_char, c_int) -> c_int>,
    seekfn: Option<unsafe extern "C" fn(*mut c_void, i64, c_int) -> i64>,
    closefn: Option<unsafe extern "C" fn(*mut c_void) -> c_int>,
) -> R<*mut File<E>> {
    let mut flags = F_LAZYBUF;
    match (readfn, writefn) {
        (None, None) => return Err(Fail::from_int(EINVAL)),
        (None, Some(_)) => flags |= F_NORD,
        (Some(_), None) => flags |= F_NOWR,
        (Some(_), Some(_)) => {}
    }
    let pointer = crate::raw::into_raw(alloc::boxed::Box::new(File::new(state)));
    let file: &'static File<E> = crate::raw::file_ref(pointer);
    let st = file.st_mut();
    st.fd = -1;
    st.flags = flags;
    st.lbf = -1;
    st.backend = crate::backend::Backend::Fun(FunState {
        cookie: RawPtr::new(cookie),
        io: FunIo {
            read: readfn,
            write: writefn,
            seek: seekfn,
            close: closefn,
        },
    });
    st.buffer = Buffer::Heap(alloc::vec![0u8; UNGET]);
    st.buf_size = 0;
    st.clear_read();
    st.clear_write();
    crate::ofl::add(state, file);
    Ok(pointer)
}

#[cfg(test)]
#[path = "funopen_ut.rs"]
mod tests;
