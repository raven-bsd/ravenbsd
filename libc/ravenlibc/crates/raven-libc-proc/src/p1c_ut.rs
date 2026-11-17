// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the thin process wrappers.

use super::*;
use alloc::vec::Vec;

use crate::backend::{ProcBackend, WaitResult};
use crate::errors::{ECHILD, ENOSYS, ESRCH};
use crate::ids::SetCred;
use crate::request::{ExecRequest, ForkRequest, SpawnRequest};

struct Fake {
    pgid: i32,
    set: core::cell::Cell<Option<(i32, i32)>>,
}

impl ProcBackend for Fake {
    fn getpid(&self) -> i32 {
        self.pgid
    }
    fn getppid(&self) -> i32 {
        1
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
    fn getpgid(&self, _pid: i32) -> Result<i32, Errno> {
        Ok(self.pgid)
    }
    fn getsid(&self, _pid: i32) -> Result<i32, Errno> {
        Ok(self.pgid)
    }
    fn getgroups(&self, _out: &mut [u32]) -> Result<usize, Errno> {
        Ok(0)
    }
    fn spawn(&self, _req: &SpawnRequest) -> Result<i32, Errno> {
        Err(ENOSYS)
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
    fn setpgid(&self, pid: i32, pgid: i32) -> Result<(), Errno> {
        self.set.set(Some((pid, pgid)));
        Ok(())
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
        _resource: u32,
        _new: Option<(u64, u64)>,
    ) -> Result<(u64, u64), Errno> {
        Err(ENOSYS)
    }
    fn rusage(&self, _who: i32) -> Result<[u8; 144], Errno> {
        Err(ENOSYS)
    }
    fn prio(&self, _which: u32, _who: i32, _set: Option<i32>) -> Result<i32, Errno> {
        Err(ENOSYS)
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

#[test]
fn setpgrp_sets_the_group_to_the_pid() {
    let backend = Fake {
        pgid: 7,
        set: core::cell::Cell::new(None),
    };
    assert_eq!(setpgrp(&backend), Ok(7));
    assert_eq!(backend.set.get(), Some((0, 0)));
}
