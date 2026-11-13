// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 1990 The Regents of the University of California.
// All rights reserved.
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/stdlib/system.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
// (file revision tag: $OpenBSD: system.c,v 1.24 2016/10/08 00:37:59 millert Exp $)
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

//! The `system()` signal/wait protocol.
//!
//! The spawn primitive is `SystemBackend::spawn_sh` (PG-45 / `posix_spawn`); until one exists the
//! shell returns `-1`/`ENOSYS` (RD-20). The protocol is the donor's: block `SIGCHLD|SIGINT|
//! SIGQUIT`, spawn **before** installing the ignores (ignored signals are inherited across
//! `exec`), ignore `SIGINT/SIGQUIT`, unblock them, wait retrying `EINTR`, then restore
//! (A-stdlib-11).

use raven_libc_abi::errno::{EINTR, Errno};

/// The signals the protocol names.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(test, derive(Debug))]
pub enum Sig {
    /// `SIGCHLD`.
    Chld,
    /// `SIGINT`.
    Int,
    /// `SIGQUIT`.
    Quit,
}

/// The process and signal seam.
pub trait SystemBackend {
    /// Block the given signals (`sigprocmask(SIG_BLOCK, ...)`).
    fn block(&mut self, s: &[Sig]);
    /// Spawn `/bin/sh -c cmd` and return its pid, with the current signal mask inherited.
    fn spawn_sh(&mut self, cmd: &[u8]) -> Result<i32, Errno>;
    /// Install `SIG_IGN` for the given signals.
    fn ignore(&mut self, s: &[Sig]);
    /// Unblock `SIGINT`/`SIGQUIT` on the first call; on the second, restore the saved mask and
    /// the saved `SIGINT`/`SIGQUIT` actions.
    fn unblock_and_restore(&mut self);
    /// `waitpid(pid, ...)`; returns the wait status.
    fn wait(&mut self, pid: i32) -> Result<i32, Errno>;
}

/// `system(command)`; `None` returns 1 without touching signals.
pub fn system<B: SystemBackend>(backend: &mut B, cmd: Option<&[u8]>) -> i32 {
    let Some(cmd) = cmd else {
        return 1;
    };

    backend.block(&[Sig::Chld, Sig::Int, Sig::Quit]);
    let pid = match backend.spawn_sh(cmd) {
        Ok(pid) => pid,
        Err(_) => {
            backend.unblock_and_restore();
            return -1;
        }
    };

    // RavenBSD: spawn first, then ignore, so the child inherits the original dispositions.
    backend.ignore(&[Sig::Int, Sig::Quit]);
    backend.unblock_and_restore();

    let status = loop {
        match backend.wait(pid) {
            Ok(status) => break status,
            Err(error) if error.0 == EINTR as u16 => continue,
            Err(_) => break -1,
        }
    };

    backend.unblock_and_restore();
    status
}

#[cfg(test)]
#[path = "system_ut.rs"]
mod tests;
