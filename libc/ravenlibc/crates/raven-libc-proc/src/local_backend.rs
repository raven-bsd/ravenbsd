// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The early-boot `ProcBackend` used before `procd` exists.

use alloc::vec::Vec;

use crate::backend::{EarlySpawn, ProcBackend, WaitResult};
use crate::errors::{ECHILD, ENOSYS, ESRCH, Errno};
use crate::ids::SetCred;
use crate::rlimit_model::RlimitTable;
use crate::wire::{ExecRequest, ForkRequest, SpawnRequest};

/// The pre-procd backend.
#[derive(Default)]
pub struct LocalBackend<'a> {
    /// Optional initfs spawner supplied by the shell.
    pub early: Option<&'a dyn EarlySpawn>,
}

impl<'a> LocalBackend<'a> {
    /// Create a backend with an optional early spawner.
    #[must_use]
    pub const fn new(early: Option<&'a dyn EarlySpawn>) -> Self {
        Self { early }
    }
}

impl ProcBackend for LocalBackend<'_> {
    fn getpid(&self) -> i32 {
        1
    }

    fn getppid(&self) -> i32 {
        0
    }

    fn getuid(&self) -> u32 {
        0
    }

    fn geteuid(&self) -> u32 {
        0
    }

    fn getgid(&self) -> u32 {
        0
    }

    fn getegid(&self) -> u32 {
        0
    }

    fn getpgid(&self, pid: i32) -> Result<i32, Errno> {
        if pid == 0 || pid == 1 {
            Ok(1)
        } else {
            Err(ESRCH)
        }
    }

    fn getsid(&self, pid: i32) -> Result<i32, Errno> {
        if pid == 0 || pid == 1 {
            Ok(1)
        } else {
            Err(ESRCH)
        }
    }

    fn getgroups(&self, _out: &mut [u32]) -> Result<usize, Errno> {
        Ok(0)
    }

    fn spawn(&self, req: &SpawnRequest) -> Result<i32, Errno> {
        let Some(early) = self.early else {
            return Err(ENOSYS);
        };
        early.spawn_initfs(&req.path, &req.argv, &req.envp, &req.fdmap)
    }

    fn fork(&self, _req: &ForkRequest) -> Result<i32, Errno> {
        Err(ENOSYS)
    }

    fn exec(&self, _req: &ExecRequest) -> Errno {
        ENOSYS
    }

    fn wait(&self, _idtype: u32, _id: i64, _options: u32) -> Result<WaitResult, Errno> {
        Err(ECHILD)
    }

    fn kill(&self, _pid: i64, _sig: u32, _queue_value: Option<u64>) -> Result<(), Errno> {
        Err(ESRCH)
    }

    fn setpgid(&self, _pid: i32, _pgid: i32) -> Result<(), Errno> {
        Err(ENOSYS)
    }

    fn setsid(&self) -> Result<i32, Errno> {
        Err(ENOSYS)
    }

    fn setcred(&self, _req: &SetCred) -> Result<(), Errno> {
        Err(ENOSYS)
    }

    fn rlimit(
        &self,
        _pid: i32,
        resource: u32,
        new: Option<(u64, u64)>,
    ) -> Result<(u64, u64), Errno> {
        if new.is_some() {
            return Err(ENOSYS);
        }
        RlimitTable::new().get(resource)
    }

    fn rusage(&self, _who: i32) -> Result<[u8; 144], Errno> {
        Ok([0; 144])
    }

    fn prio(&self, _which: u32, _who: i32, set: Option<i32>) -> Result<i32, Errno> {
        match set {
            Some(_) => Err(ENOSYS),
            None => Ok(0),
        }
    }

    fn sched(
        &self,
        _op: u32,
        _pid: i32,
        _policy: i32,
        _param: [i32; 4],
    ) -> Result<(i32, [i32; 4]), Errno> {
        Err(ENOSYS)
    }

    fn set_title(&self, _title: &[u8]) -> Result<(), Errno> {
        Err(ENOSYS)
    }

    fn prctl(&self, _option: u32, _a: [u64; 4]) -> Result<i64, Errno> {
        Err(ENOSYS)
    }

    fn login(&self, _set: Option<&[u8]>) -> Result<Vec<u8>, Errno> {
        Err(ENOSYS)
    }

    fn pledge(&self, _promises: Option<&[u8]>, _exec_promises: Option<&[u8]>) -> Result<(), Errno> {
        Err(ENOSYS)
    }

    fn cap_enter(&self) -> Result<(), Errno> {
        Err(ENOSYS)
    }
}

#[cfg(test)]
#[path = "local_backend_ut.rs"]
mod tests;
