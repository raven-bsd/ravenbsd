// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/dirent/fdopendir.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
// (no per-file header upstream; covered by musl/COPYRIGHT, H-1)
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

//! `fdopendir`: `fstat`, reject `O_PATH`, require a directory; takes ownership.
//!
//! The `FD_CLOEXEC` call of the donor is a shell `fcntl` hook (doc 07 5.7), not this crate.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::{EBADF, ENOTDIR};
use raven_libc_abi::generated::fcntl::O_PATH;
use raven_libc_abi::generated::stat::{S_IFDIR, S_IFMT};

use crate::dirent::DirStream;
use crate::{DirSys, Fd};

impl DirStream {
    /// Wrap an owned descriptor that names a directory.
    ///
    /// On failure the descriptor stays with the caller (the donor does not close it).
    ///
    /// # Errors
    ///
    /// `EBADF` for an `O_PATH` descriptor, `ENOTDIR` for a non-directory, or the backend's error.
    pub fn fdopendir<S: DirSys>(sys: &mut S, fd: Fd) -> Result<Self, Errno> {
        let stat = sys.fstat(fd)?;
        if sys.fcntl_getfl(fd)? & O_PATH != 0 {
            return Err(Errno(EBADF as u16));
        }
        if stat.st_mode & S_IFMT != S_IFDIR {
            return Err(Errno(ENOTDIR as u16));
        }
        Ok(Self::with_fd(fd))
    }
}
