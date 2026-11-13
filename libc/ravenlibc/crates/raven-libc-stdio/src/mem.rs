// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/fmemopen.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! Memory streams (`fmemopen.c`, `open_memstream.c`, `open_wmemstream.c`).
//!
//! `fmemopen` uses a caller or crate-owned byte region with the POSIX 2008 NUL rules: `r`
//! exposes the whole region, `w` starts empty, `a` starts at the first NUL, `+` modes write a
//! NUL at offset 0 and keep the region NUL terminated on every write; a `b` mode does not
//! disturb the bytes. `open_memstream` keeps `*buf`/`*size` current on every write and never
//! grows the reported size on a bare seek; `open_wmemstream` is born wide (R63) and stores
//! UTF-8 bytes decoded with the per-cookie `MbState`.
//!
//! Where musl differs from the NetBSD/FreeBSD tests on a POSIX 2008 corner case the test wins
//! (R51); the deviations are listed in the report. The constructors set `fd = -1`, the
//! orientation and `F_NORD`/`F_NOWR` from the mode, and add the FILE to the open-file list.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

// The cookie offsets are the donor's `size_t` arithmetic; every write is bounded by `size`.

use alloc::vec::Vec;

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EINVAL, ENOMEM, ENOSPC, EOVERFLOW};
use raven_libc_ctype::{MbMode, MbState, Mbr, mbrtowc};

use crate::file::{Buffer, F_EOF, F_ERR, F_LAZYBUF, F_NORD, F_NOWR, File, FileState, UNGET};
use crate::raw::{RawBuf, RawPtr, store_ptr, store_usize};
use crate::state::State;
use crate::traits::Env;
use crate::{Fail, R};

/// One memory-stream cookie.
pub(crate) struct MemCookie {
    kind: MemKind,
}

/// The three memory-stream shapes.
enum MemKind {
    /// `fmemopen`: a fixed byte region.
    Fmem {
        pos: usize,
        len: usize,
        size: usize,
        data: MemData,
        mode: u8,
        plus: bool,
    },
    /// `open_memstream`: a growing byte buffer published to the caller.
    Mstream {
        bufp: RawPtr<*mut u8>,
        sizep: RawPtr<usize>,
        pos: usize,
        len: usize,
        space: usize,
        data: Vec<u8>,
    },
    /// `open_wmemstream`: a growing wide buffer published to the caller.
    Wstream {
        bufp: RawPtr<*mut u32>,
        sizep: RawPtr<usize>,
        pos: usize,
        len: usize,
        space: usize,
        data: Vec<u32>,
        mbs: MbState,
    },
}

/// The byte storage of a `fmemopen` cookie.
enum MemData {
    /// A caller region.
    Fixed(RawBuf),
    /// A crate-owned region.
    Owned(Vec<u8>),
}

impl MemData {
    fn bytes(&self) -> &[u8] {
        match self {
            Self::Fixed(raw) => raw.bytes(),
            Self::Owned(data) => data.as_slice(),
        }
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        match self {
            Self::Fixed(raw) => raw.bytes_mut(),
            Self::Owned(data) => data.as_mut_slice(),
        }
    }
}

impl MemCookie {
    /// `fmemopen`.
    pub(crate) fn fmem(buffer: Option<(*mut u8, usize)>, size: usize, mode: &[u8]) -> R<Self> {
        let Some(&first) = mode.first() else {
            return Err(Fail::from_int(EINVAL));
        };
        if !matches!(first, b'r' | b'w' | b'a') {
            return Err(Fail::from_int(EINVAL));
        }
        let plus = mode.contains(&b'+');
        let (data, size) = match buffer {
            Some((pointer, length)) => {
                let usable = size.min(length);
                (MemData::Fixed(RawBuf::from_parts(pointer, usable)), usable)
            }
            None => {
                if size > isize::MAX as usize {
                    return Err(Fail::from_int(ENOMEM));
                }
                let region = alloc::vec![0u8; size];
                (MemData::Owned(region), size)
            }
        };
        let mut cookie = Self {
            kind: MemKind::Fmem {
                pos: 0,
                len: 0,
                size,
                data,
                mode: first,
                plus,
            },
        };
        if let MemKind::Fmem {
            pos,
            len,
            data,
            mode,
            plus: plus_flag,
            ..
        } = &mut cookie.kind
        {
            match *mode {
                b'r' => *len = *pos,
                b'a' => {
                    let data = data.bytes();
                    let mut at = 0usize;
                    while at < size && data.get(at).copied().unwrap_or(0) != 0 {
                        at += 1;
                    }
                    *len = at;
                    *pos = at;
                }
                _ => {
                    if *plus_flag {
                        if let Some(slot) = data.bytes_mut().get_mut(0) {
                            *slot = 0;
                        }
                    }
                }
            }
            if *mode == b'r' {
                *len = size;
            }
        }
        Ok(cookie)
    }

    /// `open_memstream`.
    pub(crate) fn mstream(bufp: *mut *mut u8, sizep: *mut usize) -> R<Self> {
        if bufp.is_null() || sizep.is_null() {
            return Err(Fail::from_int(EINVAL));
        }
        let mut data = Vec::new();
        if data.try_reserve_exact(1).is_err() {
            return Err(Fail::from_int(ENOMEM));
        }
        data.push(0);
        store_ptr(bufp, data.as_mut_ptr());
        store_usize(sizep, 0);
        Ok(Self {
            kind: MemKind::Mstream {
                bufp: RawPtr::new(bufp),
                sizep: RawPtr::new(sizep),
                pos: 0,
                len: 0,
                space: 0,
                data,
            },
        })
    }

    /// `open_wmemstream`.
    pub(crate) fn wstream(bufp: *mut *mut u32, sizep: *mut usize) -> R<Self> {
        if bufp.is_null() || sizep.is_null() {
            return Err(Fail::from_int(EINVAL));
        }
        let mut data = Vec::new();
        if data.try_reserve_exact(1).is_err() {
            return Err(Fail::from_int(ENOMEM));
        }
        data.push(0);
        store_ptr(bufp.cast::<*mut u8>(), data.as_mut_ptr().cast::<u8>());
        store_usize(sizep, 0);
        Ok(Self {
            kind: MemKind::Wstream {
                bufp: RawPtr::new(bufp),
                sizep: RawPtr::new(sizep),
                pos: 0,
                len: 0,
                space: 0,
                data,
                mbs: MbState::INITIAL,
            },
        })
    }

    /// Flush the FILE's pending write buffer through the cookie.
    fn flush_pending<E: Env>(&mut self, st: &mut FileState<E>) -> bool {
        let pending = st.wpos.saturating_sub(st.wbase);
        if pending == 0 {
            return true;
        }
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
        ok
    }

    /// The donor's cookie write, without the FILE pending buffer.
    fn write_cookie<E: Env>(&mut self, st: &mut FileState<E>, buf: &[u8]) -> usize {
        match &mut self.kind {
            MemKind::Fmem {
                pos,
                len,
                size,
                data,
                mode,
                ..
            } => {
                if *mode == b'a' {
                    *pos = *len;
                }
                let space = size.saturating_sub(*pos);
                let count = buf.len().min(space);
                if count < buf.len() {
                    st.flags |= F_ERR;
                    st.wpos = 0;
                    st.wbase = 0;
                    st.wend = 0;
                }
                let region = data.bytes_mut();
                if let Some(slot) = region.get_mut(*pos..*pos + count) {
                    if let Some(source) = buf.get(..count) {
                        slot.copy_from_slice(source);
                    }
                }
                *pos += count;
                if *pos > *len {
                    *len = *pos;
                    if *len < *size {
                        if let Some(slot) = data.bytes_mut().get_mut(*len) {
                            *slot = 0;
                        }
                    }
                }
                count
            }
            MemKind::Mstream {
                bufp,
                sizep,
                pos,
                len,
                space,
                data,
            } => {
                if buf.len().saturating_add(*pos) >= *space {
                    let needed = 2usize
                        .saturating_mul(*space)
                        .saturating_add(1)
                        .max(pos.saturating_add(buf.len()).saturating_add(1));
                    let extra = needed.saturating_sub(*space);
                    if data.try_reserve_exact(extra).is_err() {
                        return 0;
                    }
                    data.resize(needed, 0);
                    store_ptr(bufp.get(), data.as_mut_ptr());
                    *space = needed;
                }
                if let Some(slot) = data.get_mut(*pos..*pos + buf.len()) {
                    slot.copy_from_slice(buf);
                }
                *pos += buf.len();
                if *pos > *len {
                    *len = *pos;
                }
                store_usize(sizep.get(), (*pos).min(*len));
                buf.len()
            }
            MemKind::Wstream {
                bufp,
                sizep,
                pos,
                len,
                space,
                data,
                mbs,
            } => {
                let chars = match decode_utf8(mbs, buf) {
                    Ok(chars) => chars,
                    Err(()) => return 0,
                };
                if chars.len().saturating_add(*pos) >= *space {
                    let needed = 2usize
                        .saturating_mul(*space)
                        .saturating_add(1)
                        .max(pos.saturating_add(chars.len()).saturating_add(1));
                    if needed > isize::MAX as usize / 4 {
                        return 0;
                    }
                    let extra = needed.saturating_sub(*space);
                    if data.try_reserve_exact(extra).is_err() {
                        return 0;
                    }
                    data.resize(needed, 0);
                    store_ptr(bufp.get().cast::<*mut u8>(), data.as_mut_ptr().cast::<u8>());
                    *space = needed;
                }
                if let Some(slot) = data.get_mut(*pos..*pos + chars.len()) {
                    slot.copy_from_slice(&chars);
                }
                *pos += chars.len();
                if *pos > *len {
                    *len = *pos;
                }
                store_usize(sizep.get(), (*pos).min(*len));
                buf.len()
            }
        }
    }

    /// The backend read.
    pub(crate) fn read<E: Env>(
        &mut self,
        st: &mut FileState<E>,
        buf: &mut [u8],
    ) -> Result<usize, raven_libc_abi::errno::Errno> {
        match &mut self.kind {
            MemKind::Fmem { pos, len, data, .. } => {
                let mut remaining = len.saturating_sub(*pos);
                let mut count = buf.len();
                if count > remaining {
                    count = remaining;
                    st.flags |= F_EOF;
                }
                if let (Some(source), Some(sink)) =
                    (data.bytes().get(*pos..*pos + count), buf.get_mut(..count))
                {
                    sink.copy_from_slice(source);
                }
                *pos += count;
                remaining = remaining.saturating_sub(count);
                let extra = remaining.min(st.buf_size);
                {
                    let base = UNGET;
                    let region = st.buffer.bytes_mut();
                    if let Some(sink) = region.get_mut(base..base + extra) {
                        if let Some(source) = data.bytes().get(*pos..*pos + extra) {
                            sink.copy_from_slice(source);
                        }
                    }
                    st.rpos = base;
                    st.rend = base + extra;
                }
                *pos += extra;
                st.off = *pos as i64;
                Ok(count)
            }
            MemKind::Mstream { .. } | MemKind::Wstream { .. } => {
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
    ) -> Result<usize, raven_libc_abi::errno::Errno> {
        if !self.flush_pending(st) {
            return Err(Errno(ENOSPC as u16));
        }
        let count = self.write_cookie(st, buf);
        if matches!(self.kind, MemKind::Fmem { .. }) && count < buf.len() {
            st.wpos = 0;
            st.wbase = 0;
            st.wend = 0;
            st.flags |= F_ERR;
            if count == 0 {
                return Err(Errno(ENOSPC as u16));
            }
        }
        if let MemKind::Mstream { pos, .. } | MemKind::Wstream { pos, .. } = &self.kind {
            st.off = *pos as i64;
        }
        Ok(count)
    }

    /// The backend seek.
    pub(crate) fn seek<E: Env>(
        &mut self,
        st: &mut FileState<E>,
        off: i64,
        whence: i32,
    ) -> Result<i64, raven_libc_abi::errno::Errno> {
        if !(0..=2).contains(&whence) {
            return Err(Errno(EINVAL as u16));
        }
        match &mut self.kind {
            MemKind::Fmem { pos, len, size, .. } => {
                let base = match whence {
                    0 => 0i64,
                    1 => *pos as i64,
                    _ => *len as i64,
                };
                let new = base.checked_add(off).ok_or(Errno(EOVERFLOW as u16))?;
                let limit = *size as i64;
                if new < 0 || new > limit {
                    return Err(Errno(EINVAL as u16));
                }
                *pos = new as usize;
                st.off = new;
                Ok(new)
            }
            MemKind::Mstream {
                pos, len, sizep, ..
            } => {
                let base = match whence {
                    0 => 0i64,
                    1 => *pos as i64,
                    _ => *len as i64,
                };
                let new = base.checked_add(off).ok_or(Errno(EOVERFLOW as u16))?;
                if new < 0 || new > isize::MAX as i64 - (base.max(0)) {
                    return Err(Errno(EINVAL as u16));
                }
                *pos = new as usize;
                store_usize(sizep.get(), (*pos).min(*len));
                st.off = new;
                Ok(new)
            }
            MemKind::Wstream {
                pos,
                len,
                sizep,
                mbs,
                ..
            } => {
                let base = match whence {
                    0 => 0i64,
                    1 => *pos as i64,
                    _ => *len as i64,
                };
                let new = base.checked_add(off).ok_or(Errno(EOVERFLOW as u16))?;
                if new < 0 || new > (isize::MAX as i64 / 4) {
                    return Err(Errno(EINVAL as u16));
                }
                *mbs = MbState::INITIAL;
                *pos = new as usize;
                store_usize(sizep.get(), (*pos).min(*len));
                st.off = new;
                Ok(new)
            }
        }
    }

    /// The backend close: publish the final buffer pointer (`open_memstream`).
    pub(crate) fn close<E: Env>(
        &mut self,
        _st: &mut FileState<E>,
    ) -> Result<(), raven_libc_abi::errno::Errno> {
        match &mut self.kind {
            MemKind::Mstream {
                bufp,
                sizep,
                pos,
                len,
                data,
                ..
            } => {
                // The NetBSD `t_open_memstream` expectation: after `fclose` the reported
                // size is the final position. musl's `ms_close` does nothing; the donor test
                // wins (R51). The buffer ownership moves to the caller (trap S8): leak the
                // box so `*buf` stays valid until the caller frees it.
                let boxed = core::mem::take(data).into_boxed_slice();
                let pointer = alloc::boxed::Box::into_raw(boxed).cast::<u8>();
                store_ptr(bufp.get(), pointer);
                store_usize(sizep.get(), (*pos).min(*len));
                Ok(())
            }
            MemKind::Wstream {
                bufp,
                sizep,
                pos,
                len,
                data,
                ..
            } => {
                let boxed = core::mem::take(data).into_boxed_slice();
                let pointer = alloc::boxed::Box::into_raw(boxed).cast::<u32>();
                store_ptr(bufp.get().cast::<*mut u8>(), pointer.cast::<u8>());
                store_usize(sizep.get(), (*pos).min(*len));
                Ok(())
            }
            MemKind::Fmem { .. } => Ok(()),
        }
    }
}

/// Decode a UTF-8 byte run into code units, keeping a partial character in `state`.
fn decode_utf8(state: &mut MbState, input: &[u8]) -> Result<Vec<u32>, ()> {
    let mut out = Vec::new();
    for byte in input {
        match mbrtowc(state, MbMode::Utf8, &[*byte]) {
            Mbr::Char { wc, .. } => out.push(wc),
            Mbr::Incomplete => {}
            Mbr::Ilseq => {
                *state = MbState::INITIAL;
                return Err(());
            }
        }
    }
    Ok(out)
}

/// Build the FILE shell of a memory stream.
fn build<E: Env>(
    state: &'static State<E>,
    cookie: MemCookie,
    flags: u32,
    buf_size: usize,
    wide: bool,
) -> &'static File<E> {
    let pointer = crate::raw::into_raw(alloc::boxed::Box::new(File::new(state)));
    let file: &'static File<E> = crate::raw::file_ref(pointer);
    let st = file.st_mut();
    st.fd = -1;
    st.flags = flags | F_LAZYBUF;
    st.lbf = -1;
    st.mode = if wide { 1 } else { 0 };
    st.backend = crate::backend::Backend::Mem(cookie);
    let region = UNGET.saturating_add(buf_size);
    st.buffer = Buffer::Heap(alloc::vec![0u8; region]);
    st.buf_size = buf_size;
    st.clear_read();
    st.clear_write();
    crate::ofl::add(state, file);
    file
}

/// `fmemopen`.
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL` for a bad mode, `ENOMEM` when the region cannot be allocated.
pub fn fmemopen<E: Env>(
    state: &'static State<E>,
    buffer: Option<(*mut u8, usize)>,
    size: usize,
    mode: &[u8],
) -> R<*mut File<E>> {
    let cookie = MemCookie::fmem(buffer, size, mode)?;
    let first = mode.first().copied().unwrap_or(b'r');
    let plus = mode.contains(&b'+');
    let flags = if !plus {
        if first == b'r' { F_NOWR } else { F_NORD }
    } else {
        0
    };
    let file = build(state, cookie, flags, 1024, false);
    Ok((file as *const File<E>).cast_mut())
}

/// `open_memstream`.
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL` for a null output slot, `ENOMEM`.
pub fn open_memstream<E: Env>(
    state: &'static State<E>,
    bufp: *mut *mut u8,
    sizep: *mut usize,
) -> R<*mut File<E>> {
    let cookie = MemCookie::mstream(bufp, sizep)?;
    let file = build(state, cookie, F_NORD, crate::buf::BUFSIZ, false);
    file.st_mut().mode = -1;
    Ok((file as *const File<E>).cast_mut())
}

/// `open_wmemstream`, born wide (R63).
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL` for a null output slot, `ENOMEM`.
pub fn open_wmemstream<E: Env>(
    state: &'static State<E>,
    bufp: *mut *mut u32,
    sizep: *mut usize,
) -> R<*mut File<E>> {
    let cookie = MemCookie::wstream(bufp, sizep)?;
    // The wide stream has no byte buffer (musl `buf_size = 0`); the pushback area remains.
    let file = build(state, cookie, F_NORD, 0, true);
    file.st_mut().flags &= !F_LAZYBUF;
    Ok((file as *const File<E>).cast_mut())
}

#[cfg(test)]
#[path = "mem_ut.rs"]
mod tests;
