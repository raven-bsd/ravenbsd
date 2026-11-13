// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/getdelim.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! Line I/O (`fgets.c`, `fgetln.c`, `getdelim.c`, `getline.c`, `fputs.c`, `puts.c`).
//!
//! `getdelim` grows the caller's buffer by doubling (`realloc` becomes `Vec::try_reserve`),
//! returns the length **excluding** the NUL, reports `EINVAL` for a missing buffer,
//! `EOVERFLOW` above `SSIZE_MAX`, and returns a partial last line at end of input while
//! setting `F_EOF`; with no data at all it fails without clearing the caller's pointer (T15).
//! `fgetln` keeps its growing line in `getln_buf`, which `fclose` frees.
//!
//! [`LineBuf`] is the safe form of the `char **lineptr, size_t *n` pair: the shell wraps its
//! buffer, the core overwrites it from offset 0 and the shell copies the result back out.

#![allow(clippy::arithmetic_side_effects, clippy::indexing_slicing)]
// The donor's indices over the line buffer and the read window are reproduced directly.

use alloc::vec::Vec;

use raven_libc_abi::generated::errno::{EBADF, ENOMEM, EOVERFLOW};

use crate::file::{F_EOF, F_ERR, File};
use crate::lock::FileGuard;
use crate::state::{State, StdKind};
use crate::traits::Env;
use crate::{Fail, R};

/// The maximum value of `ssize_t` on the LP64 targets.
const SSIZE_MAX: usize = isize::MAX as usize;

/// The caller's line buffer (`char **lineptr`, `size_t *n`).
pub struct LineBuf<'a> {
    data: &'a mut Vec<u8>,
    capacity: usize,
}

impl<'a> LineBuf<'a> {
    /// Wrap the shell's buffer; `capacity` is the current `*n` (0 when the pointer is null).
    pub fn new(data: &'a mut Vec<u8>, capacity: usize) -> Self {
        Self { data, capacity }
    }

    /// The line including its NUL terminator.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        self.data.as_slice()
    }

    /// The current `*n`.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// The buffer contents without the NUL.
    #[must_use]
    pub fn line(&self) -> &[u8] {
        let len = self.data.len().saturating_sub(1);
        self.data.get(..len).unwrap_or(&[])
    }
}

/// Copy up to `k` bytes from the read window into `out` at `offset`; return the copy length
/// and whether the window contained the delimiter.
fn copy_window<E: Env>(
    file: &File<E>,
    out: &mut [u8],
    offset: usize,
    k: usize,
    delim: u8,
) -> (usize, bool) {
    let st = file.st_mut();
    let rpos = st.rpos;
    let region = st.buffer.bytes();
    let window = region.get(rpos..st.rend).unwrap_or(&[]);
    let at = window.iter().position(|byte| *byte == delim);
    let line_len = at.map_or(window.len(), |at| at + 1);
    let available = line_len.min(k).min(window.len());
    let copied = if let Some(sink) = out.get_mut(offset..offset + available) {
        if let Some(source) = window.get(..available) {
            sink.copy_from_slice(source);
        }
        available
    } else {
        0
    };
    st.rpos = rpos + copied;
    (copied, at.is_some_and(|at| at < available))
}

/// `getdelim`: read through `delim` into `line`.
///
/// Returns the number of bytes read (excluding the NUL). `line.capacity()` is the resulting
/// `*n`.
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL` when the capacity is nonsense, `ENOMEM` when the buffer
/// cannot grow, `EOVERFLOW` when the length would exceed `SSIZE_MAX`; [`Fail::Eof`] at end
/// of input with nothing read.
pub fn getdelim<E: Env>(file: &File<E>, line: &mut LineBuf<'_>, delim: i32) -> R<usize> {
    let delim = delim as u8;
    let _guard = FileGuard::new(file);
    file.st_mut().orient_byte();
    if file.st().mode == 1 {
        file.st_mut().flags |= F_ERR;
        return Err(Fail::from_int(EBADF));
    }
    if line.capacity > SSIZE_MAX {
        return Err(Fail::from_int(EOVERFLOW));
    }
    let mut n = line.capacity;
    line.data.clear();
    if n != 0 {
        if line.data.try_reserve_exact(n).is_err() {
            return Err(Fail::from_int(ENOMEM));
        }
        line.data.resize(n, 0);
    }
    let mut i = 0usize;
    loop {
        let has_window = {
            let st = file.st();
            st.rpos != st.rend
        };
        let (k, found) = if has_window {
            let st = file.st();
            let window = st.buffer.bytes().get(st.rpos..st.rend).unwrap_or(&[]);
            match window.iter().position(|byte| *byte == delim) {
                Some(at) => (at + 1, true),
                None => (window.len(), false),
            }
        } else {
            (0, false)
        };
        let needed = i.saturating_add(k);
        if needed >= n {
            let mut m = needed.saturating_add(2);
            if !found && m < usize::MAX / 4 {
                m = m.saturating_add(m / 2);
            }
            if m > SSIZE_MAX {
                file.st_mut().flags |= F_ERR;
                return Err(Fail::from_int(EOVERFLOW));
            }
            let extra = m.saturating_sub(line.data.len());
            if line.data.try_reserve_exact(extra).is_err() {
                let fit = n.saturating_sub(i);
                let (copied, _) = copy_window(file, line.data.as_mut_slice(), i, fit, delim);
                let _ = copied;
                file.st_mut().flags |= F_ERR;
                return Err(Fail::from_int(ENOMEM));
            }
            line.data.resize(m, 0);
            n = m;
            line.capacity = m;
        }
        if k != 0 {
            let (copied, saw) = copy_window(file, line.data.as_mut_slice(), i, k, delim);
            i = i.saturating_add(copied);
            if saw {
                break;
            }
            if copied < k {
                break;
            }
        }
        if found {
            break;
        }
        match crate::char_io::getc_unlocked(file) {
            Ok(c) => {
                if i.saturating_add(1) >= n {
                    let rpos = file.st().rpos;
                    if rpos != 0 {
                        file.st_mut().rpos = rpos - 1;
                        let region = file.st_mut().buffer.bytes_mut();
                        if let Some(slot) = region.get_mut(rpos - 1) {
                            *slot = c as u8;
                        }
                    }
                } else {
                    line.data[i] = c as u8;
                    i = i.saturating_add(1);
                    if c as u8 == delim {
                        break;
                    }
                }
            }
            Err(Fail::Eof) => {
                if i == 0 || file.st().flags & F_EOF == 0 {
                    return Err(Fail::Eof);
                }
                break;
            }
            Err(error) => return Err(error),
        }
    }
    line.data.truncate(i);
    line.data.push(0);
    Ok(i)
}

/// `getline`: `getdelim` with `'\n'`.
pub fn getline<E: Env>(file: &File<E>, line: &mut LineBuf<'_>) -> R<usize> {
    getdelim(file, line, i32::from(b'\n'))
}

/// `fgets`: read at most `dest.len() - 1` bytes, stop after a newline, NUL-terminate.
///
/// Returns the number of bytes stored, excluding the NUL.
///
/// # Errors
/// [`Fail::Eof`] at end of input with nothing read, or on a read error.
pub fn fgets<E: Env>(file: &File<E>, dest: &mut [u8]) -> R<usize> {
    let n = dest.len();
    let _guard = FileGuard::new(file);
    file.st_mut().orient_byte();
    if file.st().mode == 1 {
        file.st_mut().flags |= F_ERR;
        return Err(Fail::from_int(EBADF));
    }
    if n < 1 {
        return Err(Fail::Eof);
    }
    if n == 1 {
        dest[0] = 0;
        return Ok(0);
    }
    let mut stored = 0usize;
    let mut left = n - 1;
    while left != 0 {
        let has_window = {
            let st = file.st();
            st.rpos != st.rend
        };
        if has_window {
            let (copied, saw) = copy_window(file, dest, stored, left, b'\n');
            stored += copied;
            left -= copied;
            if saw || left == 0 || copied == 0 {
                break;
            }
            continue;
        }
        match crate::char_io::getc_unlocked(file) {
            Ok(c) => {
                dest[stored] = c as u8;
                stored += 1;
                left -= 1;
                if c as u8 == b'\n' {
                    break;
                }
            }
            Err(Fail::Eof) => {
                if stored == 0 || file.st().flags & F_EOF == 0 {
                    return Err(Fail::Eof);
                }
                break;
            }
            Err(error) => return Err(error),
        }
    }
    dest[stored] = 0;
    Ok(stored)
}

/// `fputs`: write a byte string.
///
/// # Errors
/// [`Fail::Eof`] when the write was short; the backend sets `F_ERR`.
pub fn fputs<E: Env>(file: &File<E>, text: &[u8]) -> R<()> {
    let len = text.len();
    let _guard = FileGuard::new(file);
    let written = crate::block_io::fwritex(file, text)?;
    if written == len {
        Ok(())
    } else {
        Err(Fail::Eof)
    }
}

/// `puts`: `fputs` plus a newline under one lock hold.
pub fn puts<E: Env>(state: &'static State<E>, text: &[u8]) -> R<()> {
    let Some(file) = state.std_file(StdKind::Out) else {
        return Err(Fail::from_int(EBADF));
    };
    let _guard = FileGuard::new(file);
    let written = crate::block_io::fwritex(file, text)?;
    if written != text.len() {
        return Err(Fail::Eof);
    }
    let _ = crate::char_io::putc_unlocked(file, i32::from(b'\n'))?;
    Ok(())
}

/// `fgetln`: the next line, copied out of the FILE's own line buffer.
///
/// Returns the line including its newline (if any) but not a NUL.
///
/// # Errors
/// [`Fail::Eof`] at end of input with nothing read.
pub fn fgetln<E: Env>(file: &File<E>) -> R<Vec<u8>> {
    let mut line = Vec::new();
    let length = {
        let _guard = FileGuard::new(file);
        // The donor primes the buffer with a getc/ungetc pair so `rend` is valid.
        if let Ok(c) = crate::char_io::getc_unlocked(file) {
            let _ = crate::ungetc::ungetc(file, c);
        }
        let mut buf = LineBuf::new(&mut line, 0);
        getdelim(file, &mut buf, i32::from(b'\n'))?
    };
    line.truncate(length);
    file.st_mut().getln_buf = Some(line.clone());
    Ok(line)
}

#[cfg(test)]
#[path = "line_io_ut.rs"]
mod tests;
