// SPDX-License-Identifier: BSD-2-Clause
// Copyright (c) 2008 Ed Schouten <ed@FreeBSD.org>
// All rights reserved.
//
// RavenBSD: transpiled from freebsd:lib/libc/gen/posix_spawn.c @ 133344405f532bd54ff3629a77d4c4e062f01fdb
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE AUTHOR AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE AUTHOR OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! The `posix_spawn` attribute and file-action API.

use alloc::vec::Vec;

use crate::consts::POSIX_SPAWN_ALL;
use crate::errors::{E2BIG, EBADF, EINVAL, ENAMETOOLONG, Errno};
use crate::limits::{MAX_FILE_ACTIONS, PATH_MAX};

/// The `posix_spawnattr_t` model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnAttr {
    /// `POSIX_SPAWN_*` flags.
    pub flags: i16,
    /// Process group for `SETPGROUP`.
    pub pgroup: i32,
    /// Signals reset to the default action.
    pub sigdefault: [u8; 128],
    /// Signal mask for the child.
    pub sigmask: [u8; 128],
    /// Scheduler policy.
    pub sched_policy: i32,
    /// Scheduler priority.
    pub sched_priority: i32,
}

impl SpawnAttr {
    /// A zeroed attribute set.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            flags: 0,
            pgroup: 0,
            sigdefault: [0; 128],
            sigmask: [0; 128],
            sched_policy: 0,
            sched_priority: 0,
        }
    }

    /// Set the flag word, rejecting unknown bits.
    pub fn set_flags(&mut self, flags: i16) -> Result<(), Errno> {
        if flags & !POSIX_SPAWN_ALL != 0 {
            return Err(EINVAL);
        }
        self.flags = flags;
        Ok(())
    }

    /// The flag word.
    #[must_use]
    pub const fn flags(&self) -> i16 {
        self.flags
    }

    /// Set the process group.
    pub const fn set_pgroup(&mut self, pgroup: i32) {
        self.pgroup = pgroup;
    }

    /// The process group.
    #[must_use]
    pub const fn pgroup(&self) -> i32 {
        self.pgroup
    }

    /// Set the default-signal set.
    pub const fn set_sigdefault(&mut self, sigdefault: [u8; 128]) {
        self.sigdefault = sigdefault;
    }

    /// The default-signal set.
    #[must_use]
    pub const fn sigdefault(&self) -> [u8; 128] {
        self.sigdefault
    }

    /// Set the child signal mask.
    pub const fn set_sigmask(&mut self, sigmask: [u8; 128]) {
        self.sigmask = sigmask;
    }

    /// The child signal mask.
    #[must_use]
    pub const fn sigmask(&self) -> [u8; 128] {
        self.sigmask
    }

    /// Set the scheduler policy.
    pub const fn set_sched_policy(&mut self, policy: i32) {
        self.sched_policy = policy;
    }

    /// The scheduler policy.
    #[must_use]
    pub const fn sched_policy(&self) -> i32 {
        self.sched_policy
    }

    /// Set the scheduler priority.
    pub const fn set_sched_priority(&mut self, priority: i32) {
        self.sched_priority = priority;
    }

    /// The scheduler priority.
    #[must_use]
    pub const fn sched_priority(&self) -> i32 {
        self.sched_priority
    }
}

impl Default for SpawnAttr {
    fn default() -> Self {
        Self::new()
    }
}

/// One file action.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FileAction {
    /// Open `path` and place it at `fd`.
    Open {
        /// Destination descriptor.
        fd: i32,
        /// Path to open.
        path: Vec<u8>,
        /// Open flags.
        oflag: i32,
        /// Creation mode.
        mode: u32,
    },
    /// Duplicate `fd` to `newfd`.
    Dup2 {
        /// Source descriptor.
        fd: i32,
        /// Destination descriptor.
        newfd: i32,
    },
    /// Close `fd`.
    Close {
        /// Descriptor.
        fd: i32,
    },
    /// Change the scratch current directory.
    Chdir {
        /// New directory.
        path: Vec<u8>,
    },
    /// Change the scratch current directory through an open descriptor.
    Fchdir {
        /// Directory descriptor.
        fd: i32,
    },
    /// Drop every descriptor at or above `from`.
    CloseFrom {
        /// First descriptor to close.
        from: i32,
    },
    /// Remember a controlling-terminal descriptor.
    TcSetPgrp {
        /// Terminal descriptor.
        fd: i32,
    },
}

/// An ordered list of file actions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileActions {
    actions: Vec<FileAction>,
}

impl FileActions {
    /// An empty action list.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    /// The actions in append order.
    #[must_use]
    pub fn actions(&self) -> &[FileAction] {
        &self.actions
    }

    /// Number of actions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Whether the list is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    fn room(&self) -> Result<(), Errno> {
        if self.actions.len() >= MAX_FILE_ACTIONS {
            Err(E2BIG)
        } else {
            Ok(())
        }
    }

    /// Append `OPEN`.
    pub fn add_open(&mut self, fd: i32, path: &[u8], oflag: i32, mode: u32) -> Result<(), Errno> {
        check_fd(fd)?;
        if path.len() > PATH_MAX {
            return Err(ENAMETOOLONG);
        }
        self.room()?;
        self.actions.push(FileAction::Open {
            fd,
            path: path.to_vec(),
            oflag,
            mode,
        });
        Ok(())
    }

    /// Append `DUP2`.
    pub fn add_dup2(&mut self, fd: i32, newfd: i32) -> Result<(), Errno> {
        check_fd(fd)?;
        check_fd(newfd)?;
        self.room()?;
        self.actions.push(FileAction::Dup2 { fd, newfd });
        Ok(())
    }

    /// Append `CLOSE`.
    pub fn add_close(&mut self, fd: i32) -> Result<(), Errno> {
        check_fd(fd)?;
        self.room()?;
        self.actions.push(FileAction::Close { fd });
        Ok(())
    }

    /// Append `CHDIR`.
    pub fn add_chdir(&mut self, path: &[u8]) -> Result<(), Errno> {
        if path.len() > PATH_MAX {
            return Err(ENAMETOOLONG);
        }
        self.room()?;
        self.actions.push(FileAction::Chdir {
            path: path.to_vec(),
        });
        Ok(())
    }

    /// Append `FCHDIR`.
    pub fn add_fchdir(&mut self, fd: i32) -> Result<(), Errno> {
        check_fd(fd)?;
        self.room()?;
        self.actions.push(FileAction::Fchdir { fd });
        Ok(())
    }

    /// Append `CLOSEFROM`.
    pub fn add_closefrom(&mut self, from: i32) -> Result<(), Errno> {
        check_fd(from)?;
        self.room()?;
        self.actions.push(FileAction::CloseFrom { from });
        Ok(())
    }

    /// Append `TCSETPGRP`.
    pub fn add_tcsetpgrp(&mut self, fd: i32) -> Result<(), Errno> {
        check_fd(fd)?;
        self.room()?;
        self.actions.push(FileAction::TcSetPgrp { fd });
        Ok(())
    }
}

fn check_fd(fd: i32) -> Result<(), Errno> {
    if fd < 0 { Err(EBADF) } else { Ok(()) }
}

#[cfg(test)]
#[path = "api_ut.rs"]
mod tests;
