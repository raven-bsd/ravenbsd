// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/clearerr.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! The status functions (`clearerr.c`, `feof.c`, `ferror.c`, `fileno.c`).
//!
//! The indicators are bits of the FILE flags; `fileno` on a memory or cookie stream fails
//! with `EBADF` (R47).

use raven_libc_abi::generated::errno::EBADF;

use crate::file::{F_EOF, F_ERR, File};
use crate::lock::FileGuard;
use crate::traits::Env;
use crate::{Fail, R};

/// `clearerr`: clear both indicators.
pub fn clearerr<E: Env>(file: &File<E>) {
    let _guard = FileGuard::new(file);
    file.st_mut().flags &= !(F_EOF | F_ERR);
}

/// `feof`.
#[must_use]
pub fn feof<E: Env>(file: &File<E>) -> bool {
    let _guard = FileGuard::new(file);
    file.st().flags & F_EOF != 0
}

/// `ferror`.
#[must_use]
pub fn ferror<E: Env>(file: &File<E>) -> bool {
    let _guard = FileGuard::new(file);
    file.st().flags & F_ERR != 0
}

/// `fileno`.
///
/// # Errors
/// [`Fail::Errno`] with `EBADF` when the stream has no descriptor.
pub fn fileno<E: Env>(file: &File<E>) -> R<i32> {
    let _guard = FileGuard::new(file);
    let fd = file.st().fd;
    if fd < 0 {
        Err(Fail::from_int(EBADF))
    } else {
        Ok(fd)
    }
}

#[cfg(test)]
#[path = "status_ut.rs"]
mod tests;
