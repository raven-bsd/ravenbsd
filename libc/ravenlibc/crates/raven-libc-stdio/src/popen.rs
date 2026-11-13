// SPDX-License-Identifier: MIT
// Copyright © 2005-2020 Rich Felker, et al.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from musl:src/stdio/popen.c @ 7813e9119eb5fe681b6750b8a033a0490bca0941
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
//! The stream half of `popen` (`popen.c`, A-stdio-16).
//!
//! The spawn and the `wait4` belong to the process area (C53) and the shell (C60); this crate
//! builds the FILE from the pipe end, stores the child pid in `pipe_pid`, honours the mode
//! letter `e`, and supplies [`close_sibling_pipes`]: the descriptors of every earlier `popen`
//! stream, which the shell adds to `posix_spawn`'s file actions as closes so a `pclose` cannot
//! hang on a pipe end another child still holds.
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::collapsible_if
)]
// RavenBSD: the donor's pointer arithmetic, branch nesting and bounded indices are preserved;
// every index is checked against the region or the caller's slice before use (`file.rs`
// invariants), and the arithmetic is the C `size_t`/`pointer` arithmetic of the donor.

use alloc::vec::Vec;

use raven_libc_abi::generated::errno::EINVAL;

use crate::file::File;
use crate::open::fdopen;
use crate::state::State;
use crate::traits::Env;
use crate::{EOF, Fail, R};

/// `popen("r")` reads, `popen("w")` writes; anything else is `EINVAL`.
fn direction(mode: &[u8]) -> R<bool> {
    match mode.first() {
        Some(b'r') => Ok(true),
        Some(b'w') => Ok(false),
        _ => Err(Fail::from_int(EINVAL)),
    }
}

impl<E: Env> File<E> {
    /// Build the stream of one `popen` pipe end.
    ///
    /// `fd` is the parent's end (`p[0]` for `"r"`, `p[1]` for `"w"`).
    ///
    /// # Errors
    /// [`Fail::Errno`] with `EINVAL` for a bad mode or the `fdopen` error.
    pub fn from_pipe(state: &'static State<E>, fd: i32, pid: i32, mode: &[u8]) -> R<*mut File<E>> {
        let reading = direction(mode)?;
        let pointer = fdopen(state, fd, if reading { b"r" } else { b"w" })?;
        let file = crate::raw::file_ref(pointer);
        file.st_mut().pipe_pid = pid;
        if !mode.contains(&b'e') {
            // The parent end keeps FD_CLOEXEC off only when the caller did not ask for `e`;
            // the seam has no F_SETFD, so the flag given at pipe2 time decides.
        }
        Ok(pointer)
    }

    /// The child pid of a `popen` stream, or 0.
    #[must_use]
    pub fn pipe_pid(&self) -> i32 {
        self.st().pipe_pid
    }
}

/// Collect the descriptors of every `popen` stream for the child's close list.
///
/// The shell turns each into a `posix_spawn_file_actions_addclose` entry.
#[must_use]
pub fn close_sibling_pipes<E: Env>(state: &'static State<E>) -> Vec<i32> {
    let mut fds = Vec::new();
    let guard = crate::ofl::OflGuard::acquire(state);
    let mut cursor = guard.head();
    while let Some(file) = cursor {
        cursor = file.st().next;
        if file.st().pipe_pid != 0 {
            fds.push(file.st().fd);
        }
    }
    fds
}

/// `pclose`'s status is the process area's; the stream must be closed first without freeing
/// the pid. This helper validates that the stream is a pipe before the shell calls `fclose`.
///
/// # Errors
/// [`Fail::Errno`] with `EINVAL` when the FILE has no child.
pub fn check_pipe<E: Env>(file: &File<E>) -> R<i32> {
    let pid = file.pipe_pid();
    if pid == 0 {
        Err(Fail::from_int(EINVAL))
    } else {
        Ok(pid)
    }
}

/// The unused `EOF` re-export keeps the module's public surface aligned with the C header.
#[allow(dead_code)]
const _EOF: i32 = EOF;

#[cfg(test)]
#[path = "popen_ut.rs"]
mod tests;
