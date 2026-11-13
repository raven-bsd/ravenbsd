// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/fopen.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! The open family (`fopen.c`, `__fdopen.c`, `freopen.c`, `__fmodeflags.c`).
//!
//! The mode string's first byte selects `r`/`w`/`a`; `+`, `b`, `e` and `x` may follow in any
//! order and `x` adds `O_EXCL` (T8). A first byte outside `rwa` is `EINVAL`. `fdopen`
//! rejects a mode the descriptor cannot serve (`EINVAL`), sets `F_APP` and
//! `O_APPEND` for `a`, and every new FILE carries `F_LAZYBUF`: the buffer is allocated at the
//! first read or write (`crate::buf::smakebuf_locked`), not in the FILE allocation as musl
//! does. `freopen` without a path re-applies the status flags; with a path it opens a new
//! descriptor and adopts it (the `dup3` of the donor is not part of the [`crate::SysFile`]
//! seam, so the fd number changes; the stream behaves identically).
//!
//! `__fopen_rb_ca` builds a read-only FILE in caller storage with no allocation: the resolver
//! (C51/C63) needs it.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use alloc::boxed::Box;
use alloc::vec::Vec;

use raven_libc_abi::generated::errno::{EINVAL, ENOMEM};
use raven_libc_abi::generated::fcntl::{
    O_APPEND, O_CLOEXEC, O_CREAT, O_EXCL, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY,
};

use crate::file::{Buffer, F_APP, F_LAZYBUF, F_NORD, F_NOWR, F_PERM, File, UNGET};
use crate::lock::FileGuard;
use crate::raw::RawBuf;
use crate::state::State;
use crate::traits::Env;
use crate::{Fail, R};

/// The parsed mode string.
#[derive(Clone, Copy)]
struct Mode {
    /// The first byte: `r`, `w` or `a`.
    first: u8,
    /// `+` present.
    plus: bool,
    /// `e` present.
    close_on_exec: bool,
    /// The `open(2)` flags.
    flags: i32,
}

impl Mode {
    /// Whether the FILE may read.
    fn readable(self) -> bool {
        self.plus || self.first == b'r'
    }

    /// Whether the FILE may write.
    fn writable(self) -> bool {
        self.plus || self.first != b'r'
    }
}

/// Parse and validate the mode string (`__fmodeflags`).
fn parse_mode(mode: &[u8]) -> R<Mode> {
    let Some(&first) = mode.first() else {
        return Err(Fail::from_int(EINVAL));
    };
    if !matches!(first, b'r' | b'w' | b'a') {
        return Err(Fail::from_int(EINVAL));
    }
    let has = |needle: u8| mode.contains(&needle);
    let plus = has(b'+');
    let exclusive = has(b'x');
    let close_on_exec = has(b'e');
    let mut flags = if plus {
        O_RDWR
    } else if first == b'r' {
        O_RDONLY
    } else {
        O_WRONLY
    };
    if exclusive {
        flags |= O_EXCL;
    }
    if close_on_exec {
        flags |= O_CLOEXEC;
    }
    if first != b'r' {
        flags |= O_CREAT;
    }
    if first == b'w' {
        flags |= O_TRUNC;
    }
    if first == b'a' {
        flags |= O_APPEND;
    }
    Ok(Mode {
        first,
        plus,
        close_on_exec,
        flags,
    })
}

/// The flags of a new FILE.
fn stream_flags(mode: Mode) -> u32 {
    let mut flags = F_LAZYBUF;
    if !mode.plus {
        if mode.first == b'r' {
            flags |= F_NOWR;
        } else {
            flags |= F_NORD;
        }
    }
    if mode.first == b'a' {
        flags |= F_APP;
    }
    flags
}

/// Allocate a heap FILE and publish its pointer.
fn allocate<E: Env>(state: &'static State<E>) -> (&'static File<E>, *mut File<E>) {
    let pointer = crate::raw::into_raw(Box::new(File::new(state)));
    let file: &'static File<E> = crate::raw::file_ref(pointer);
    (file, pointer)
}

/// Give a fresh FILE its descriptor state.
fn init_fd<E: Env>(file: &File<E>, fd: i32, mode: Mode) {
    let st = file.st_mut();
    st.fd = fd;
    st.flags = stream_flags(mode);
    st.lbf = -1;
    st.mode = 0;
    st.lock = core::sync::atomic::AtomicI32::new(0);
    st.backend = crate::backend::Backend::Fd;
    st.buffer = Buffer::Heap(Vec::from([0u8; UNGET]));
    st.buf_size = 0;
    st.probe = false;
    st.clear_read();
    st.clear_write();
}

/// `fopen`.
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL` for a bad mode, `ENOMEM` when the FILE cannot be allocated,
/// or the platform `open` error.
pub fn fopen<E: Env>(state: &'static State<E>, path: &[u8], mode: &[u8]) -> R<*mut File<E>> {
    let parsed = parse_mode(mode)?;
    let fd = match state.env().open(path, parsed.flags, 0o666) {
        Ok(fd) => fd,
        Err(error) => return Err(Fail::Errno(error)),
    };
    let (file, pointer) = allocate(state);
    init_fd(file, fd, parsed);
    crate::ofl::add(state, file);
    Ok(pointer)
}

/// `fdopen`.
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL` for a bad mode or a descriptor that cannot serve it, or
/// `ENOMEM`.
pub fn fdopen<E: Env>(state: &'static State<E>, fd: i32, mode: &[u8]) -> R<*mut File<E>> {
    let parsed = parse_mode(mode)?;
    let access = match state.env().getfl(fd) {
        Ok(flags) => flags & 3,
        Err(error) => return Err(Fail::Errno(error)),
    };
    let readable = access != O_WRONLY;
    let writable = access != O_RDONLY;
    if (parsed.readable() && !readable) || (parsed.writable() && !writable) {
        return Err(Fail::from_int(EINVAL));
    }
    if parsed.first == b'a' {
        match state.env().getfl(fd) {
            Ok(flags) => {
                if flags & O_APPEND == 0 {
                    if let Err(error) = state.env().setfl(fd, flags | O_APPEND) {
                        return Err(Fail::Errno(error));
                    }
                }
            }
            Err(error) => return Err(Fail::Errno(error)),
        }
    }
    let (file, pointer) = allocate(state);
    init_fd(file, fd, parsed);
    crate::ofl::add(state, file);
    Ok(pointer)
}

/// `freopen`.
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL`, the platform `open`/`fcntl` error, or the flushed stream's
/// error. On failure the stream is closed, as the donor does.
pub fn freopen<E: Env>(
    state: &'static State<E>,
    path: Option<&[u8]>,
    mode: &[u8],
    file: &'static File<E>,
) -> R<*mut File<E>> {
    let parsed = parse_mode(mode)?;
    let guard = FileGuard::new(file);
    let _ = crate::flush::fflush(state, Some(file));
    let permanent = file.st().flags & F_PERM;
    if let Some(path) = path {
        let fd = match state.env().open(path, parsed.flags, 0o666) {
            Ok(fd) => fd,
            Err(error) => {
                drop(guard);
                let _ = crate::close::fclose(file);
                return Err(Fail::Errno(error));
            }
        };
        if let Err(error) = crate::backend::close(file) {
            let _ = error;
        }
        init_fd(file, fd, parsed);
        file.st_mut().flags |= permanent;
        Ok((file as *const File<E>).cast_mut())
    } else {
        let flags = parsed.flags & !(O_CREAT | O_EXCL | O_CLOEXEC);
        let fd = file.st().fd;
        // The seam has no F_SETFD; on the no-path form O_CLOEXEC is not re-applied.
        if parsed.close_on_exec {
            // RavenBSD: no F_SETFD method in SysFile; the open-time flag was cleared above.
        }
        if let Err(error) = state.env().setfl(fd, flags) {
            drop(guard);
            let _ = crate::close::fclose(file);
            return Err(Fail::Errno(error));
        }
        file.st_mut().mode = 0;
        Ok((file as *const File<E>).cast_mut())
    }
}

/// `__fopen_rb_ca`: a read-only FILE in caller storage.
///
/// # Errors
/// The platform `open` error.
pub fn fopen_rb_ca<E: Env>(
    state: &'static State<E>,
    path: &[u8],
    file: &'static File<E>,
    buffer: (*mut u8, usize),
) -> R<()> {
    let fd = match state.env().open(path, O_RDONLY | O_CLOEXEC, 0) {
        Ok(fd) => fd,
        Err(error) => return Err(Fail::Errno(error)),
    };
    let (pointer, length) = buffer;
    if length <= UNGET {
        let _ = state.env().close(fd);
        return Err(Fail::from_int(ENOMEM));
    }
    let st = file.st_mut();
    st.fd = fd;
    st.flags = F_NOWR | F_PERM;
    st.buf_size = length - UNGET;
    st.buffer = Buffer::Fixed(RawBuf::from_parts(pointer, length));
    st.lbf = -1;
    st.mode = 0;
    st.backend = crate::backend::Backend::Fd;
    st.clear_read();
    st.clear_write();
    Ok(())
}

#[cfg(test)]
#[path = "open_ut.rs"]
mod tests;
