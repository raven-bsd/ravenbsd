// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/dirent/readdir.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `readdir`: refill with `getdents`, parse one record.
//!
//! `getdents` returning `ENOENT` is end of stream (`A-dir-1`, a directory removed while open);
//! a malformed record is `EIO` (`RD-new`). Unlocked: the shell owns the `DIR` lock (`A-dir-3`).

use raven_libc_abi::errno::Errno;
use raven_libc_abi::generated::errno::ENOENT;

use crate::DirSys;
use crate::dirent::{DirEntry, DirStream, parse_dirent};

impl DirStream {
    /// Return the next entry, or `None` at the end of the stream.
    ///
    /// # Errors
    ///
    /// The backend's `errno` value, or `EIO` for a malformed `getdents` record.
    pub fn readdir<S: DirSys>(&mut self, sys: &mut S) -> Result<Option<DirEntry>, Errno> {
        if self.buf_pos >= self.buf_end {
            let len = match sys.getdents(self.fd, &mut self.buf) {
                Ok(len) => len,
                Err(errno) if errno.0 == ENOENT as u16 => return Ok(None),
                Err(errno) => return Err(errno),
            };
            if len == 0 {
                return Ok(None);
            }
            self.buf_end = len.min(self.buf.len());
            self.buf_pos = 0;
        }
        let (entry, next) = parse_dirent(&self.buf, self.buf_pos)?;
        self.buf_pos = next;
        self.tell = entry.off;
        Ok(Some(entry))
    }
}
