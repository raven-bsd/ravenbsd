// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/fread.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! Block I/O (`musl:src/stdio/fread.c`, `fwrite.c` and `__fwritex`).
//!
//! `fread` first exhausts the read window, then reads the remainder straight into the
//! caller's buffer for large requests and refills for the tail. `fwrite` returns items, not
//! bytes, and `fwrite(p, 0, n)` / `fwrite(p, n, 0)` return 0 without touching the stream
//! (T14). `__fwritex` is the unlocked internal used by `fwrite`, `FileSink` and `fputs`.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

// The donor's pointer arithmetic over `dest`/`rpos`/`wpos` is reproduced with indices; every
// slice bound is checked before the access.

use raven_libc_abi::generated::errno::{EBADF, EIO, EOVERFLOW};

use crate::file::{F_ERR, File};
use crate::lock::FileGuard;
use crate::traits::Env;
use crate::{Fail, R};

/// `fread(ptr, size, nmemb, f)`: return the number of complete items read.
///
/// # Errors
/// [`Fail::Errno`] with `EOVERFLOW` when `size * nmemb` does not fit.
pub fn fread<E: Env>(file: &File<E>, dest: &mut [u8], size: usize, nmemb: usize) -> R<usize> {
    if size == 0 {
        return Ok(0);
    }
    let Some(len) = size.checked_mul(nmemb) else {
        return Err(Fail::from_int(EOVERFLOW));
    };
    let len = len.min(dest.len());
    let _guard = FileGuard::new(file);
    file.st_mut().orient_byte();
    if file.st().mode == 1 {
        file.st_mut().flags |= F_ERR;
        return Err(Fail::from_int(EBADF));
    }
    let mut done = 0usize;
    let buffered = {
        let st = file.st();
        if st.rpos != st.rend {
            Some((st.rend - st.rpos).min(len))
        } else {
            None
        }
    };
    if let Some(count) = buffered {
        let region = file.st().buffer.bytes();
        let rpos = file.st().rpos;
        if let Some(source) = region.get(rpos..rpos + count) {
            dest[..count].copy_from_slice(source);
        }
        file.st_mut().rpos += count;
        done += count;
    }
    while done < len {
        crate::buf::flush_before_read(file);
        if crate::mode::toread(file).unwrap_or(true) {
            break;
        }
        let remaining = len - done;
        match crate::backend::read(file, &mut dest[done..done + remaining]) {
            Ok(0) => break,
            Ok(count) => done += count,
            Err(_) => break,
        }
    }
    Ok(done / size)
}

/// `__fwritex`: the unlocked writer.
///
/// Returns the number of bytes of `data` accepted. A short count is valid.
///
/// # Errors
/// The backend error; `F_ERR` is set by the backend.
pub(crate) fn fwritex<E: Env>(file: &File<E>, data: &[u8]) -> R<usize> {
    let len = data.len();
    if file.st().wend == 0 && crate::mode::towrite(file).is_err() {
        // The donor's `__fwritex` returns 0 when `__towrite` fails; `errno` is untouched
        // (the C `fwrite` reports a short count, `fputs` returns -1).
        return Ok(0);
    }
    let space = {
        let st = file.st();
        st.wend.saturating_sub(st.wpos)
    };
    if len > space {
        return match crate::backend::write(file, data) {
            Ok(count) => Ok(count),
            Err(error) => Err(Fail::Errno(error)),
        };
    }
    if file.st().lbf >= 0 {
        let mut split = len;
        while split != 0 && data.get(split - 1) != Some(&b'\n') {
            split -= 1;
        }
        if split != 0 {
            let count = match crate::backend::write(file, data.get(..split).unwrap_or(&[])) {
                Ok(count) => count,
                Err(error) => return Err(Fail::Errno(error)),
            };
            if count < split {
                return Ok(count);
            }
            let tail = fwritex(file, data.get(split..).unwrap_or(&[]))?;
            return Ok(split + tail);
        }
    }
    let st = file.st_mut();
    let region = st.buffer.bytes_mut();
    if let Some(slot) = region.get_mut(st.wpos..) {
        if let Some(slot) = slot.get_mut(..len) {
            slot.copy_from_slice(data);
            st.wpos += len;
            return Ok(len);
        }
    }
    Err(Fail::from_int(EIO))
}

/// `fwrite(src, size, nmemb, f)`: return the number of complete items written.
///
/// # Errors
/// [`Fail::Errno`] with `EOVERFLOW` when `size * nmemb` does not fit; a backend error is
/// reported as a short count with `F_ERR`, exactly as the donor does.
pub fn fwrite<E: Env>(file: &File<E>, source: &[u8], size: usize, nmemb: usize) -> R<usize> {
    if size == 0 {
        return Ok(0);
    }
    let Some(len) = size.checked_mul(nmemb) else {
        return Err(Fail::from_int(EOVERFLOW));
    };
    let len = len.min(source.len());
    let _guard = FileGuard::new(file);
    // The C returns a short count and sets `F_ERR` (and usually `errno`); the count is what
    // the caller observes. The shell maps `Ok(0)` to 0, never to -1.
    let written = fwritex(file, source.get(..len).unwrap_or(&[])).unwrap_or(0);
    Ok(written / size)
}

#[cfg(test)]
#[path = "block_io_ut.rs"]
mod tests;
