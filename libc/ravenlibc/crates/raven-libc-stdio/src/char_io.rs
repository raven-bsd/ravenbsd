// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/__uflow.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! Byte-at-a-time I/O (`__uflow.c`, `__overflow.c`, the `getc.h`/`putc.h` fast paths,
//! `getc.c`, `putc.c`, `fgetc.c`, `fputc.c`, `getchar*.c`, `putchar*.c`).
//!
//! `getc_unlocked` reads `*rpos++` when the window is non-empty and falls back to `__uflow`,
//! which flushes line-buffered writers first (A-stdio-13), switches to read mode, and reads one
//! byte. `putc_unlocked` stores into the write window unless the byte is the line-buffer
//! newline or the window is full, in which case `__overflow` writes it. The locking forms use
//! [`FileGuard`]; the `_unlocked` forms never lock (their caller holds it).
//!
//! A write error is sticky (`F_ERR`); `clearerr` is the only reset among these functions.
//! `EINTR` is returned to the caller, never retried (NetBSD `h_intr.c`).

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The cursor arithmetic is the donor's; every index is within the region length or the
// caller's slice.

use raven_libc_abi::generated::errno::{EBADF, EIO};

use crate::file::File;
use crate::lock::FileGuard;
use crate::state::{State, StdKind};
use crate::traits::Env;
use crate::{Fail, R};

/// `getc_unlocked`: the buffered fast path or `read one byte`.
pub fn getc_unlocked<E: Env>(file: &File<E>) -> R<i32> {
    let buffered = {
        let st = file.st();
        if st.rpos != st.rend {
            Some(st.buffer.bytes().get(st.rpos).copied().unwrap_or(0))
        } else {
            None
        }
    };
    if let Some(byte) = buffered {
        file.st_mut().rpos = file.st().rpos.saturating_add(1);
        return Ok(i32::from(byte));
    }
    uflow(file)
}

/// `__uflow`: switch to read mode and read one byte into the stream.
///
/// # Errors
/// [`Fail::Eof`] at end of input; [`Fail::Errno`] with `EBADF` for a write-only or
/// wide-oriented stream and with the platform error of the failing read.
pub fn uflow<E: Env>(file: &File<E>) -> R<i32> {
    crate::buf::flush_before_read(file);
    if crate::mode::toread(file)? {
        return Err(Fail::Eof);
    }
    let mut byte = [0u8; 1];
    match crate::backend::read(file, &mut byte) {
        Ok(1) => Ok(i32::from(byte[0])),
        Ok(_) => Err(Fail::Eof),
        Err(error) => Err(Fail::Errno(error)),
    }
}

/// `__overflow`: write one byte through the stream.
///
/// # Errors
/// [`Fail::Errno`] with `EBADF` for a read-only or wide-oriented stream and with the backend
/// error otherwise.
pub fn overflow<E: Env>(file: &File<E>, c: i32) -> R<i32> {
    let byte = c as u8;
    let needs_switch = file.st().wend == 0;
    if needs_switch {
        crate::mode::towrite(file)?;
    }
    {
        let st = file.st_mut();
        if st.wpos != st.wend && i32::from(byte) != st.lbf {
            let region = st.buffer.bytes_mut();
            if let Some(slot) = region.get_mut(st.wpos) {
                *slot = byte;
                st.wpos = st.wpos.saturating_add(1);
                return Ok(i32::from(byte));
            }
        }
    }
    match crate::backend::write(file, &[byte]) {
        Ok(1) => Ok(i32::from(byte)),
        Ok(_) => Err(Fail::from_int(EIO)),
        Err(error) => Err(Fail::Errno(error)),
    }
}

/// `putc_unlocked`.
pub fn putc_unlocked<E: Env>(file: &File<E>, c: i32) -> R<i32> {
    let inline = {
        let st = file.st();
        st.wend != 0 && st.wpos != st.wend && i32::from(c as u8) != st.lbf
    };
    if inline {
        let st = file.st_mut();
        let region = st.buffer.bytes_mut();
        if let Some(slot) = region.get_mut(st.wpos) {
            *slot = c as u8;
            st.wpos = st.wpos.saturating_add(1);
            return Ok(i32::from(c as u8));
        }
    }
    overflow(file, c)
}

/// `getc` (locked).
pub fn getc<E: Env>(file: &File<E>) -> R<i32> {
    let _guard = FileGuard::new(file);
    crate::mode::require_byte(file)?;
    getc_unlocked(file)
}

/// `fgetc`.
pub fn fgetc<E: Env>(file: &File<E>) -> R<i32> {
    getc(file)
}

/// `fgetc_unlocked`.
pub fn fgetc_unlocked<E: Env>(file: &File<E>) -> R<i32> {
    getc_unlocked(file)
}

/// `putc` (locked).
pub fn putc<E: Env>(file: &File<E>, c: i32) -> R<i32> {
    let _guard = FileGuard::new(file);
    crate::mode::require_byte(file)?;
    putc_unlocked(file, c)
}

/// `fputc`.
pub fn fputc<E: Env>(file: &File<E>, c: i32) -> R<i32> {
    putc(file, c)
}

/// `fputc_unlocked`.
pub fn fputc_unlocked<E: Env>(file: &File<E>, c: i32) -> R<i32> {
    putc_unlocked(file, c)
}

/// `getchar`: `fgetc(stdin)`.
pub fn getchar<E: Env>(state: &'static State<E>) -> R<i32> {
    let Some(file) = state.std_file(StdKind::In) else {
        return Err(Fail::from_int(EBADF));
    };
    getc(file)
}

/// `getchar_unlocked`.
pub fn getchar_unlocked<E: Env>(state: &'static State<E>) -> R<i32> {
    let Some(file) = state.std_file(StdKind::In) else {
        return Err(Fail::from_int(EBADF));
    };
    crate::mode::require_byte(file)?;
    getc_unlocked(file)
}

/// `putchar`: `fputc(c, stdout)`.
pub fn putchar<E: Env>(state: &'static State<E>, c: i32) -> R<i32> {
    let Some(file) = state.std_file(StdKind::Out) else {
        return Err(Fail::from_int(EBADF));
    };
    putc(file, c)
}

/// `putchar_unlocked`.
pub fn putchar_unlocked<E: Env>(state: &'static State<E>, c: i32) -> R<i32> {
    let Some(file) = state.std_file(StdKind::Out) else {
        return Err(Fail::from_int(EBADF));
    };
    crate::mode::require_byte(file)?;
    putc_unlocked(file, c)
}

#[cfg(test)]
#[path = "char_io_ut.rs"]
mod tests;
