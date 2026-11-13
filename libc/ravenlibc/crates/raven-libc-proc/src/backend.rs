// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The `procd` client seam.

use alloc::vec::Vec;

use crate::errors::Errno;
use crate::ids::SetCred;
use crate::wire::{ExecRequest, ForkRequest, SpawnRequest};

pub use crate::wire::FdMapEntry;

/// One completed wait result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitResult {
    /// Child pid, zero for `WNOHANG` with nothing ready.
    pub pid: i32,
    /// Raw wait status.
    pub status: i32,
    /// Opaque `siginfo_t`.
    pub siginfo: [u8; 128],
    /// Opaque `rusage`.
    pub rusage: [u8; 144],
}

/// The early-boot spawn seam implemented by the shell.
pub trait EarlySpawn {
    /// Spawn one initfs program.
    fn spawn_initfs(
        &self,
        path: &[u8],
        argv: &[Vec<u8>],
        envp: &[Vec<u8>],
        fds: &[FdMapEntry],
    ) -> Result<i32, Errno>;
}

/// The process operations the models need.
pub trait ProcBackend {
    /// Current pid.
    fn getpid(&self) -> i32;
    /// Parent pid.
    fn getppid(&self) -> i32;
    /// Real uid.
    fn getuid(&self) -> u32;
    /// Effective uid.
    fn geteuid(&self) -> u32;
    /// Real gid.
    fn getgid(&self) -> u32;
    /// Effective gid.
    fn getegid(&self) -> u32;
    /// Process group of `pid`.
    fn getpgid(&self, pid: i32) -> Result<i32, Errno>;
    /// Session of `pid`.
    fn getsid(&self, pid: i32) -> Result<i32, Errno>;
    /// Supplementary groups.
    fn getgroups(&self, out: &mut [u32]) -> Result<usize, Errno>;
    /// Spawn a new image.
    fn spawn(&self, req: &SpawnRequest) -> Result<i32, Errno>;
    /// Fork the caller.
    fn fork(&self, req: &ForkRequest) -> Result<i32, Errno>;
    /// Replace the caller's image; returns only on failure.
    fn exec(&self, req: &ExecRequest) -> Errno;
    /// Wait for a child.
    fn wait(&self, idtype: u32, id: i64, options: u32) -> Result<WaitResult, Errno>;
    /// Send a signal.
    fn kill(&self, pid: i64, sig: u32, queue_value: Option<u64>) -> Result<(), Errno>;
    /// Set a process group.
    fn setpgid(&self, pid: i32, pgid: i32) -> Result<(), Errno>;
    /// Create a session.
    fn setsid(&self) -> Result<i32, Errno>;
    /// Change credentials.
    fn setcred(&self, req: &SetCred) -> Result<(), Errno>;
    /// Read or write one resource limit.
    fn rlimit(&self, pid: i32, resource: u32, new: Option<(u64, u64)>)
    -> Result<(u64, u64), Errno>;
    /// Read resource usage.
    fn rusage(&self, who: i32) -> Result<[u8; 144], Errno>;
    /// Read or write a priority.
    fn prio(&self, which: u32, who: i32, set: Option<i32>) -> Result<i32, Errno>;
    /// Scheduler operation.
    fn sched(
        &self,
        op: u32,
        pid: i32,
        policy: i32,
        param: [i32; 4],
    ) -> Result<(i32, [i32; 4]), Errno>;
    /// Set the process title.
    fn set_title(&self, title: &[u8]) -> Result<(), Errno>;
    /// Native `prctl`.
    fn prctl(&self, option: u32, a: [u64; 4]) -> Result<i64, Errno>;
    /// Login-name operation.
    fn login(&self, set: Option<&[u8]>) -> Result<Vec<u8>, Errno>;
    /// Apply a pledge.
    fn pledge(&self, promises: Option<&[u8]>, exec_promises: Option<&[u8]>) -> Result<(), Errno>;
    /// Enter capability mode.
    fn cap_enter(&self) -> Result<(), Errno>;
}

#[cfg(test)]
#[path = "backend_ut.rs"]
mod tests;
