// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/exec.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
//
// Copyright (c) 1991, 1993
//	The Regents of the University of California.  All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! The `execvpe` PATH search, generic over a try-exec closure.
//!
//! The donor's `vfork`/`mmap` workarounds are removed (`vfork` is `fork` on Raven); the search
//! order, empty-element rule, diagnostics and errno precedence are kept.

use alloc::vec::Vec;

use crate::consts::DEFPATH;
use crate::errors::{EACCES, EISDIR, ELOOP, ENAMETOOLONG, ENOENT, ENOEXEC, ENOTDIR, Errno};
use crate::limits::PATH_MAX;

/// One path candidate offered to the try-exec closure.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Attempt<'a> {
    /// Candidate path.
    pub path: &'a [u8],
    /// The closure must run `/bin/sh <path>` with the remaining arguments.
    pub shell_fallback: bool,
}

/// Search `file` in `path_env` and return the first successful closure value.
pub fn search_path<T>(
    file: &[u8],
    path_env: Option<&[u8]>,
    diag_prefix: &[u8],
    try_exec: &mut dyn FnMut(&Attempt) -> Result<T, Errno>,
    diag: &mut dyn FnMut(&[u8]),
) -> Result<T, Errno> {
    if file.is_empty() {
        return Err(ENOENT);
    }
    if file.contains(&b'/') {
        return attempt(file, try_exec);
    }
    let path = path_env.unwrap_or(DEFPATH);
    let mut eacces = false;
    for element in path.split(|byte| *byte == b':') {
        let element = if element.is_empty() {
            b".".as_slice()
        } else {
            element
        };
        let total = element
            .len()
            .checked_add(file.len())
            .and_then(|value| value.checked_add(2));
        let Some(total) = total else {
            diag_too_long(diag_prefix, element, diag);
            continue;
        };
        if total > PATH_MAX {
            diag_too_long(diag_prefix, element, diag);
            continue;
        }
        let mut candidate = Vec::with_capacity(total);
        candidate.extend_from_slice(element);
        candidate.push(b'/');
        candidate.extend_from_slice(file);
        match attempt(&candidate, try_exec) {
            Ok(value) => return Ok(value),
            Err(error) if continues(error) => {}
            Err(EACCES) => eacces = true,
            Err(error) => return Err(error),
        }
    }
    if eacces { Err(EACCES) } else { Err(ENOENT) }
}

fn continues(error: Errno) -> bool {
    matches!(error, ENOENT | EISDIR | ELOOP | ENAMETOOLONG | ENOTDIR)
}

fn attempt<T>(
    path: &[u8],
    try_exec: &mut dyn FnMut(&Attempt) -> Result<T, Errno>,
) -> Result<T, Errno> {
    match try_exec(&Attempt {
        path,
        shell_fallback: false,
    }) {
        Ok(value) => Ok(value),
        Err(ENOEXEC) => try_exec(&Attempt {
            path,
            shell_fallback: true,
        }),
        Err(error) => Err(error),
    }
}

fn diag_too_long(prefix: &[u8], element: &[u8], diag: &mut dyn FnMut(&[u8])) {
    let mut message = Vec::with_capacity(
        prefix
            .len()
            .saturating_add(element.len())
            .saturating_add(20),
    );
    message.extend_from_slice(prefix);
    message.extend_from_slice(b": ");
    message.extend_from_slice(element);
    message.extend_from_slice(b": path too long\n");
    diag(&message);
}

/// Build the shell argv for an `ENOEXEC` fallback.
#[must_use]
pub fn script_argv(argv: &[Vec<u8>], script_path: &[u8]) -> Vec<Vec<u8>> {
    let mut result = Vec::new();
    result.push(b"sh".to_vec());
    result.push(script_path.to_vec());
    if let Some(rest) = argv.get(1..) {
        result.extend_from_slice(rest);
    }
    result
}

#[cfg(test)]
#[path = "exec_path_ut.rs"]
mod tests;
