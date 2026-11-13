// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// RavenBSD: transpiled from openbsd:lib/libc/gen/daemon.c @ 4ce71f2817f675745b0596aaf04de4a122b1cc52
//
// Copyright (c) 1990, 1993
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

//! The `daemon` step model.

use crate::errors::Errno;

/// The result of the one required fork.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DaemonFork {
    /// The original parent must exit.
    Parent,
    /// The child continues.
    Child,
}

/// The step the shell must act on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DaemonStep {
    /// The shell calls `_exit(0)`.
    ParentExit,
    /// The child is ready.
    ChildReady,
}

/// The operations `daemon` needs.
pub trait DaemonOps {
    /// Fork once.
    fn fork(&mut self) -> Result<DaemonFork, Errno>;
    /// Create a session.
    fn setsid(&mut self) -> Result<(), Errno>;
    /// Change to `/`.
    fn chdir_root(&mut self) -> Result<(), Errno>;
    /// Open `/dev/null`.
    fn open_devnull(&mut self) -> Result<i32, Errno>;
    /// Duplicate a descriptor.
    fn dup2(&mut self, fd: i32, newfd: i32) -> Result<(), Errno>;
    /// Close a descriptor.
    fn close(&mut self, fd: i32) -> Result<(), Errno>;
}

/// Run the daemon sequence.
pub fn daemon(ops: &mut dyn DaemonOps, nochdir: bool, noclose: bool) -> Result<DaemonStep, Errno> {
    if ops.fork()? == DaemonFork::Parent {
        return Ok(DaemonStep::ParentExit);
    }
    ops.setsid()?;
    if !nochdir {
        ops.chdir_root()?;
    }
    if !noclose {
        let fd = ops.open_devnull()?;
        for target in 0..=2 {
            if fd != target {
                ops.dup2(fd, target)?;
            }
        }
        if fd > 2 {
            ops.close(fd)?;
        }
    }
    Ok(DaemonStep::ChildReady)
}

#[cfg(test)]
#[path = "daemon_ut.rs"]
mod tests;
