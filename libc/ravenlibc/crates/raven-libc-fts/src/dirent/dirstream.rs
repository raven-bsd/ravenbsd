// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/dirent/__dirent.h @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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

//! `struct __dirstream`: the `DIR` core (doc 07 3.6).
//!
//! The donor enforces `offsetof(struct __dirstream, buf) % sizeof(off_t) == 0`; the Rust record
//! parser does not need alignment, but the 2048-byte buffer size and the field roles are kept.
//! The lock is not here: the shell locks `DIR` for `readdir_r`, `seekdir` and `rewinddir`
//! (`A-dir-3`), exactly the musl split.

use crate::Fd;
use crate::consts::DIRSTREAM_BUFSIZE;

/// A directory stream: `{ tell, fd, buf_pos, buf_end, buf }`.
pub struct DirStream {
    /// `d_off` of the last returned entry, initially 0.
    pub(super) tell: i64,
    /// The owned descriptor.
    pub(super) fd: Fd,
    /// Current position in `buf`.
    pub(super) buf_pos: usize,
    /// End of valid data in `buf`.
    pub(super) buf_end: usize,
    /// The `getdents` buffer.
    pub(super) buf: [u8; DIRSTREAM_BUFSIZE],
}

impl DirStream {
    /// A fresh stream around an owned descriptor.
    pub(super) fn with_fd(fd: Fd) -> Self {
        Self {
            tell: 0,
            fd,
            buf_pos: 0,
            buf_end: 0,
            buf: [0; DIRSTREAM_BUFSIZE],
        }
    }

    /// Clear the buffer (the caller has positioned the descriptor).
    pub(super) fn reset_buffer(&mut self) {
        self.buf_pos = 0;
        self.buf_end = 0;
    }
}
