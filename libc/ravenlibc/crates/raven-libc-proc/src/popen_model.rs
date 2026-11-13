// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/popen.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
//
// Copyright (c) 1988, 1993
//	The Regents of the University of California.  All rights reserved.
//
// This code is derived from software written by Ken Arnold and
// published in UNIX Review, Vol. 6, No. 8.
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

//! The `popen` pid list and spawn plan.

use alloc::vec::Vec;

use crate::errors::{EINVAL, Errno};
use crate::spawn_model::api::FileActions;

/// The open `popen` children, keyed by the parent's descriptor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PidList {
    entries: Vec<(i32, i32)>,
}

impl PidList {
    /// An empty list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Remember `pid` for `fd`.
    pub fn insert(&mut self, fd: i32, pid: i32) {
        self.entries.push((fd, pid));
    }

    /// Remove and return the pid for `fd`.
    pub fn remove(&mut self, fd: i32) -> Option<i32> {
        let index = self.entries.iter().position(|(entry, _)| *entry == fd)?;
        Some(self.entries.remove(index).1)
    }

    /// The parent descriptors in insertion order.
    #[must_use]
    pub fn fds(&self) -> Vec<i32> {
        self.entries.iter().map(|(fd, _)| *fd).collect()
    }

    /// Number of open entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The direction and close-on-exec mode of a `popen` stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PopenMode {
    /// The parent reads from the child.
    pub read: bool,
    /// The parent writes to the child.
    pub write: bool,
    /// The parent descriptor is close-on-exec.
    pub cloexec: bool,
}

/// Parse a `popen` mode string.
pub fn popen_mode(mode: &[u8]) -> Result<PopenMode, Errno> {
    match mode {
        b"r" => Ok(PopenMode {
            read: true,
            write: false,
            cloexec: false,
        }),
        b"w" => Ok(PopenMode {
            read: false,
            write: true,
            cloexec: false,
        }),
        b"re" => Ok(PopenMode {
            read: true,
            write: false,
            cloexec: true,
        }),
        b"we" => Ok(PopenMode {
            read: false,
            write: true,
            cloexec: true,
        }),
        _ => Err(EINVAL),
    }
}

/// Build the file actions and argv for one `popen`.
#[must_use]
pub fn popen_plan(
    mode: PopenMode,
    parent_end: i32,
    child_end: i32,
    list: &PidList,
    cmd: &[u8],
) -> (FileActions, Vec<Vec<u8>>) {
    let mut actions = FileActions::new();
    for fd in list.fds() {
        let _ = actions.add_close(fd);
    }
    let target = if mode.read { 1 } else { 0 };
    let _ = actions.add_dup2(child_end, target);
    let _ = actions.add_close(child_end);
    let _ = actions.add_close(parent_end);
    let argv = alloc::vec![b"sh".to_vec(), b"-c".to_vec(), cmd.to_vec()];
    (actions, argv)
}

#[cfg(test)]
#[path = "popen_model_ut.rs"]
mod tests;
