// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The process request models passed to [`crate::backend::ProcBackend`].
//!
//! The kernel owns process semantics. These plain data structures carry one spawn, fork or exec
//! request across the libc backend seam; they are not a wire protocol and have no encoder.

use alloc::vec::Vec;

/// One fd-to-handle pair in a spawn or exec map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FdMapEntry {
    /// Destination file descriptor.
    pub fd: i32,
    /// Sender's handle value.
    pub handle: u32,
}

/// A request to spawn a new image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnRequest {
    /// `posix_spawn` flags mapped to the kernel spawn flags.
    pub spawn_flags: u32,
    /// Requested process group, or zero.
    pub pgroup: i32,
    /// Signals reset to their default action.
    pub sigdefault: [u8; 128],
    /// Signal mask for the child.
    pub sigmask: [u8; 128],
    /// Signals ignored by the child.
    pub ignored: [u8; 128],
    /// Scheduler policy.
    pub sched_policy: i32,
    /// Scheduler priority.
    pub sched_prio: i32,
    /// Child umask.
    pub umask: u32,
    /// Terminal foreground-group fd, or -1.
    pub tcpgrp_fd: i32,
    /// Image path.
    pub path: Vec<u8>,
    /// Child working directory.
    pub cwd: Vec<u8>,
    /// Argument vector.
    pub argv: Vec<Vec<u8>>,
    /// Environment vector.
    pub envp: Vec<Vec<u8>>,
    /// Inherited descriptor map.
    pub fdmap: Vec<FdMapEntry>,
}

/// A request to fork the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForkRequest {
    /// Entry point of the child's first thread.
    pub entry: u64,
    /// Stack pointer the child's first thread starts on.
    pub sp: u64,
    /// Argument passed to the entry point.
    pub arg: u64,
    /// Upper bound on the fork flags; non-zero is rejected in v1.
    pub fork_flags: u32,
    /// User TLS base the child's thread inherits.
    pub tp: u64,
}

/// A request to replace the caller's image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecRequest {
    /// Exec flags.
    pub exec_flags: u32,
    /// Image handle.
    pub fd_handle: u32,
    /// Image path.
    pub path: Vec<u8>,
    /// Working directory.
    pub cwd: Vec<u8>,
    /// Argument vector.
    pub argv: Vec<Vec<u8>>,
    /// Environment vector.
    pub envp: Vec<Vec<u8>>,
    /// Handles to close before the image starts.
    pub close_list: Vec<u32>,
    /// Descriptor map.
    pub fdmap: Vec<FdMapEntry>,
    /// Signal mask.
    pub sigmask: [u8; 128],
    /// Ignored signals.
    pub ignored: [u8; 128],
}
