// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/__stdio_exit.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! The exit cleanup and the fork hooks (`musl:src/stdio/__stdio_exit.c`, `fork.c:16-30`).
//!
//! `stdio_cleanup` runs after the `atexit` handlers and `.fini_array` (A-stdio-8). For every
//! FILE in the open-file list, then `stdin`, `stdout`, `stderr`, it takes the FILE lock
//! **forever** (no unlock, so a thread that still writes after exit started blocks instead of
//! racing), flushes pending output, and repositions a read stream with
//! `seek(rpos - rend, SEEK_CUR)` so a shared descriptor offset is correct for the next
//! process. It closes nothing and frees nothing. The list lock is taken and never released,
//! exactly as the donor leaves it.
//!
//! The fork triple touches only the list lock: `prepare` locks it, `parent` releases it,
//! `child` stores 0 instead of unlocking (T11 — FILE locks are deliberately untouched).
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use raven_libc_abi::generated::fcntl::SEEK_CUR;

use crate::file::File;
use crate::state::State;
use crate::std_streams::StdStreams;
use crate::traits::Env;

/// `stdio_cleanup` (`__stdio_exit`).
pub fn stdio_cleanup<E: Env>(state: &'static State<E>, streams: &'static StdStreams<E>) {
    crate::ofl::lock_list(state);
    let mut cursor = state.ofl_head();
    while let Some(file) = cursor {
        cursor = file.st().next;
        close_file(file);
    }
    for file in streams.all() {
        close_file(file);
    }
}

/// Lock one FILE forever, flush its output and reposition its read window.
fn close_file<E: Env>(file: &File<E>) {
    let _ = crate::lock::lockfile(file);
    if file.st().has_pending_write() {
        let _ = crate::backend::write(file, &[]);
    }
    if file.st().rpos != file.st().rend {
        let unread = file.st().unread();
        let unread = i64::try_from(unread).unwrap_or(0);
        let _ = crate::backend::seek(file, -unread, SEEK_CUR);
    }
}

/// The atfork `prepare` handler: lock the open-file list.
pub fn fork_prepare<E: Env>(state: &'static State<E>) {
    crate::ofl::lock_list(state);
}

/// The atfork `parent` handler: release the list lock.
pub fn fork_parent<E: Env>(state: &'static State<E>) {
    crate::ofl::unlock_list(state);
}

/// The atfork `child` handler: store 0 into the list lock (`fork.c:80`).
pub fn fork_child<E: Env>(state: &'static State<E>) {
    crate::ofl::reset_list(state);
}

#[cfg(test)]
#[path = "exit_ut.rs"]
mod tests;
