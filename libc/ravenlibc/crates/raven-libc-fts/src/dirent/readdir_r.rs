// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/dirent/readdir_r.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `readdir_r`: the deprecated reentrant form. The copy into the caller's `struct dirent` is the
//! shell's job (it owns the C layout); the lock is the shell's (`A-dir-3`).
//!
//! The C version copies `d_reclen` bytes; this crate hands out a parsed [`DirEntry`] and the
//! shell copies exactly its `reclen`.

use raven_libc_abi::errno::Errno;

use crate::DirSys;
use crate::dirent::{DirEntry, DirStream};

impl DirStream {
    /// Return the next entry; same stream state as [`DirStream::readdir`].
    ///
    /// # Errors
    ///
    /// The backend's `errno` value, or `EIO` for a malformed record.
    pub fn readdir_r<S: DirSys>(&mut self, sys: &mut S) -> Result<Option<DirEntry>, Errno> {
        self.readdir(sys)
    }
}
