// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/__stdout_write.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! Buffer policy: constants, the lazy `smakebuf`, the inverted stdout probe and the
//! flush-before-read walk.
//!
//! This module is CLEANROOM (`donor-map/04` 3.5): the buffer size policy is modelled on
//! `openbsd:lib/libc/stdio/makebuf.c` (read for behaviour, never copied) and the
//! flush-before-read rule on `netbsd:lib/libc/stdio/refill.c`'s `lflush` walk; the stdout
//! probe is musl's `__stdout_write` idea with `isatty` replacing `TIOCGWINSZ`.
//!
//! Rules: `BUFSIZ = 4096`, `UNGET = 8`, the two standard buffers `STD_BUF = 1024`, `stderr`
//! unbuffered. `fopen`/`fdopen`/`freopen`/`popen`/`tmpfile` streams carry `F_LAZYBUF`; the
//! first read or write calls `smakebuf`, which uses `st_blksize` clamped to `[512, 65536]`,
//! `BUFSIZ` on a failed `fstat`, and falls back to the unbuffered state when allocation
//! fails. `stdout` instead keeps its static buffer and turns line buffering **off** on the
//! first write when the fd is not a terminal (A-stdio-6).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use alloc::vec::Vec;

use crate::file::{F_LAZYBUF, F_SVB, File, UNGET};
use crate::lock::FileGuard;
use crate::traits::Env;

/// The advisory buffer size of the C API (`H-19`), also the fallback of `smakebuf`.
pub const BUFSIZ: usize = 4096;

/// The static buffer of `stdin` and `stdout`, excluding the [`UNGET`] prefix.
pub const STD_BUF: usize = 1024;

/// The local buffer of one unbuffered `fprintf` (`A-stdio-12`).
pub const UNBUF_PRINT_BUF: usize = 1024;

/// The smallest buffer `smakebuf` accepts.
const MIN_BUF: usize = 512;

/// The largest buffer `smakebuf` accepts.
const MAX_BUF: usize = 65_536;

/// Allocate the lazy buffer of a stream (`__smakebuf`).
///
/// The caller holds the FILE lock. `F_SVB` disables the whole policy.
pub(crate) fn smakebuf_locked<E: Env>(file: &File<E>) {
    if file.st().flags & F_SVB != 0 {
        file.st_mut().flags &= !F_LAZYBUF;
        return;
    }
    file.st_mut().flags &= !F_LAZYBUF;
    if file.st().buf_size != 0 {
        return;
    }
    let size = buffer_size(file);
    let char_dev = file
        .env()
        .fstat(file.st().fd)
        .map(|stat| stat.is_char_dev)
        .unwrap_or(false);
    let Some(region) = allocate(UNGET + size) else {
        // OpenBSD's fallback: switch to the unbuffered state instead of failing.
        file.st_mut().buf_size = 0;
        return;
    };
    {
        let st = file.st_mut();
        st.buffer.set_vec(region);
        st.buf_size = size;
    }
    if char_dev && file.env().isatty(file.st().fd) {
        file.st_mut().lbf = i32::from(b'\n');
    }
}

/// The buffer size policy of `smakebuf`/`setvbuf`: `st_blksize` clamped to `[512, 65536]`,
/// `BUFSIZ` when `fstat` fails or gives zero.
pub(crate) fn buffer_size<E: Env>(file: &File<E>) -> usize {
    match file.env().fstat(file.st().fd) {
        Ok(stat) if stat.blksize != 0 => (stat.blksize as usize).clamp(MIN_BUF, MAX_BUF),
        _ => BUFSIZ,
    }
}

/// The default heap region of a stream that has no caller buffer.
pub(crate) fn default_region(size: usize) -> Option<Vec<u8>> {
    allocate(UNGET + size)
}

/// Allocate a zeroed region; `None` when the allocator refuses.
fn allocate(len: usize) -> Option<Vec<u8>> {
    let mut region = Vec::new();
    if region.try_reserve_exact(len).is_err() {
        return None;
    }
    region.resize(len, 0);
    Some(region)
}

/// The flush-before-read rule of `donor-map/04` 4.3 rule 4.
///
/// Called at every refill point of an unbuffered or line-buffered input: every line-buffered
/// writer with pending output is flushed first, so a prompt written without `fflush` appears
/// before a blocking read (A-stdio-13). The standard streams are visited before the open-file
/// list; the list lock is held across the walk and one FILE lock at a time is taken.
pub(crate) fn flush_before_read<E: Env>(file: &File<E>) {
    let line_or_unbuffered = {
        let st = file.st();
        st.buf_size == 0 || st.lbf >= 0
    };
    if !line_or_unbuffered {
        return;
    }
    let state = file.state();
    for kind in [
        crate::state::StdKind::Out,
        crate::state::StdKind::Err,
        crate::state::StdKind::In,
    ] {
        if let Some(other) = state.std_file(kind) {
            if core::ptr::eq(other, file) {
                continue;
            }
            flush_if_line_buffered(other);
        }
    }
    let guard = crate::ofl::OflGuard::acquire(state);
    let mut cursor = guard.head();
    while let Some(other) = cursor {
        let next = other.st().next;
        if !core::ptr::eq(other, file) {
            flush_if_line_buffered(other);
        }
        cursor = next;
    }
}

/// Flush one writer when it is line buffered and has pending output.
fn flush_if_line_buffered<E: Env>(file: &File<E>) {
    if file.st().flags & crate::file::F_NOWR != 0 || file.st().lbf < 0 {
        return;
    }
    let _ = FileGuard::new(file);
    if file.st().has_pending_write() {
        let _ = crate::backend::write(file, &[]);
    }
}

#[cfg(test)]
#[path = "buf_ut.rs"]
mod tests;
