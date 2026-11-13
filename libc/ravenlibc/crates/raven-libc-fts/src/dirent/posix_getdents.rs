// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/dirent/posix_getdents.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `posix_getdents`: `flags != 0` is `EOPNOTSUPP`; `len` is clamped to `INT_MAX`.

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::EOPNOTSUPP;

use crate::DirSys;
use crate::dirent::DirStream;

impl DirStream {
    /// Raw `getdents` into `buf`.
    ///
    /// # Errors
    ///
    /// `EOPNOTSUPP` for non-zero flags, otherwise the backend's `getdents` error.
    pub fn posix_getdents<S: DirSys>(
        &mut self,
        sys: &mut S,
        buf: &mut [u8],
        flags: u32,
    ) -> Result<usize, Errno> {
        if flags != 0 {
            return Err(Errno(EOPNOTSUPP as u16));
        }
        let len = buf.len().min(i32::MAX as usize);
        let buffer = buf.get_mut(..len).unwrap_or(&mut []);
        sys.getdents(self.fd, buffer)
    }
}
