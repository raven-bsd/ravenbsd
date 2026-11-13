// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A deterministic in-memory `procd` for tests.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;
use core::cell::{Cell, RefCell};

use crate::backend::{ProcBackend, WaitResult};
use crate::consts::RLIMIT_NLIMITS;
use crate::errors::{ECHILD, ENOSYS, ESRCH, Errno};
use crate::ids::SetCred;
use crate::rlimit_model::RlimitTable;
use crate::wire::{
    ExecRequest, ForkRequest, GetGroupsRequest, GetPgidRequest, GetSidRequest, KillRequest,
    LoginRequest, Op, PledgeRequest, PrctlRequest, PrioRequest, Request, RlimitRequest,
    RusageRequest, SchedRequest, SetCredRequest, SetPgidRequest, SetTitleRequest, SpawnRequest,
    WaitRequest,
};

/// A scripted process server.
pub struct FakeProcd {
    next_pid: Cell<i32>,
    processes: RefCell<BTreeMap<i32, i32>>,
    spawn_results: RefCell<VecDeque<Result<i32, Errno>>>,
    wait_results: RefCell<VecDeque<Result<WaitResult, Errno>>>,
    failure: RefCell<Option<(Op, Errno)>>,
    log: RefCell<Vec<Request>>,
}

impl FakeProcd {
    /// Create a server whose first pid is 100.
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_pid: Cell::new(100),
            processes: RefCell::new(BTreeMap::new()),
            spawn_results: RefCell::new(VecDeque::new()),
            wait_results: RefCell::new(VecDeque::new()),
            failure: RefCell::new(None),
            log: RefCell::new(Vec::new()),
        }
    }

    /// Every request in call order.
    #[must_use]
    pub fn requests(&self) -> Vec<Request> {
        self.log.borrow().clone()
    }

    /// The last request, if any.
    #[must_use]
    pub fn last_request(&self) -> Option<Request> {
        self.log.borrow().last().cloned()
    }

    /// Script the next spawn result.
    pub fn push_spawn_result(&self, result: Result<i32, Errno>) {
        self.spawn_results.borrow_mut().push_back(result);
    }

    /// Script the next wait result.
    pub fn push_wait_result(&self, result: Result<WaitResult, Errno>) {
        self.wait_results.borrow_mut().push_back(result);
    }

    /// Fail the next operation `op`.
    pub fn fail_next(&self, op: Op, errno: Errno) {
        *self.failure.borrow_mut() = Some((op, errno));
    }

    fn log(&self, request: Request) {
        self.log.borrow_mut().push(request);
    }

    fn take_failure(&self, op: Op) -> Option<Errno> {
        let mut failure = self.failure.borrow_mut();
        match *failure {
            Some((failed, errno)) if failed == op => {
                *failure = None;
                Some(errno)
            }
            _ => None,
        }
    }

    fn allocate(&self) -> i32 {
        let pid = self.next_pid.get();
        self.next_pid.set(pid.saturating_add(1));
        self.processes.borrow_mut().insert(pid, 0);
        pid
    }
}

impl Default for FakeProcd {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcBackend for FakeProcd {
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
        self.log(Request::GetPgid(GetPgidRequest { pid }));
        if let Some(errno) = self.take_failure(Op::GetPgid) {
            return Err(errno);
        }
        if pid == 0 || pid == 1 || self.processes.borrow().contains_key(&pid) {
            Ok(1)
        } else {
            Err(ESRCH)
        }
    }

    fn getsid(&self, pid: i32) -> Result<i32, Errno> {
        self.log(Request::GetSid(GetSidRequest { pid }));
        if let Some(errno) = self.take_failure(Op::GetSid) {
            return Err(errno);
        }
        if pid == 0 || pid == 1 || self.processes.borrow().contains_key(&pid) {
            Ok(1)
        } else {
            Err(ESRCH)
        }
    }

    fn getgroups(&self, out: &mut [u32]) -> Result<usize, Errno> {
        self.log(Request::GetGroups(GetGroupsRequest {
            max: u32::try_from(out.len()).unwrap_or(u32::MAX),
        }));
        self.take_failure(Op::GetGroups).map_or(Ok(0), Err)
    }

    fn spawn(&self, req: &SpawnRequest) -> Result<i32, Errno> {
        self.log(Request::Spawn(req.clone()));
        if let Some(errno) = self.take_failure(Op::Spawn) {
            return Err(errno);
        }
        if let Some(result) = self.spawn_results.borrow_mut().pop_front() {
            return result;
        }
        Ok(self.allocate())
    }

    fn fork(&self, req: &ForkRequest) -> Result<i32, Errno> {
        self.log(Request::Fork(req.clone()));
        if let Some(errno) = self.take_failure(Op::Fork) {
            return Err(errno);
        }
        Ok(self.allocate())
    }

    fn exec(&self, req: &ExecRequest) -> Errno {
        self.log(Request::Exec(req.clone()));
        self.take_failure(Op::Exec).unwrap_or(ENOSYS)
    }

    fn wait(&self, idtype: u32, id: i64, options: u32) -> Result<WaitResult, Errno> {
        self.log(Request::Wait(WaitRequest {
            idtype,
            id,
            options,
        }));
        if let Some(errno) = self.take_failure(Op::Wait) {
            return Err(errno);
        }
        self.wait_results
            .borrow_mut()
            .pop_front()
            .unwrap_or(Err(ECHILD))
    }

    fn kill(&self, pid: i64, sig: u32, queue_value: Option<u64>) -> Result<(), Errno> {
        self.log(Request::Kill(KillRequest {
            pid,
            sig,
            flags: u32::from(queue_value.is_some()),
            value: queue_value,
        }));
        self.take_failure(Op::Kill).map_or(Ok(()), Err)
    }

    fn setpgid(&self, pid: i32, pgid: i32) -> Result<(), Errno> {
        self.log(Request::SetPgid(SetPgidRequest { pid, pgid }));
        self.take_failure(Op::SetPgid).map_or(Ok(()), Err)
    }

    fn setsid(&self) -> Result<i32, Errno> {
        self.log(Request::SetSid);
        self.take_failure(Op::SetSid).map_or(Ok(1), Err)
    }

    fn setcred(&self, req: &SetCred) -> Result<(), Errno> {
        self.log(Request::SetCred(SetCredRequest {
            kind: req.kind,
            a: req.a,
            b: req.b,
            c: req.c,
            groups: req.groups.clone(),
        }));
        self.take_failure(Op::SetCred).map_or(Ok(()), Err)
    }

    fn rlimit(
        &self,
        pid: i32,
        resource: u32,
        new: Option<(u64, u64)>,
    ) -> Result<(u64, u64), Errno> {
        let (has_new, new_cur, new_max) = match new {
            Some((cur, max)) => (1, cur, max),
            None => (0, 0, 0),
        };
        self.log(Request::Rlimit(RlimitRequest {
            pid,
            resource,
            has_new,
            new_cur,
            new_max,
        }));
        if let Some(errno) = self.take_failure(Op::Rlimit) {
            return Err(errno);
        }
        let mut table = RlimitTable::new();
        if let Some((cur, max)) = new {
            table.set(resource, cur, max, true)?;
            Ok((cur, max))
        } else if resource < RLIMIT_NLIMITS {
            table.get(resource)
        } else {
            Err(crate::errors::EINVAL)
        }
    }

    fn rusage(&self, who: i32) -> Result<[u8; 144], Errno> {
        self.log(Request::Rusage(RusageRequest { who }));
        self.take_failure(Op::Rusage).map_or(Ok([0; 144]), Err)
    }

    fn prio(&self, which: u32, who: i32, set: Option<i32>) -> Result<i32, Errno> {
        self.log(Request::Prio(PrioRequest {
            which,
            who,
            op: u32::from(set.is_some()),
            value: set.unwrap_or(0),
        }));
        if let Some(errno) = self.take_failure(Op::Prio) {
            return Err(errno);
        }
        Ok(set.unwrap_or(0))
    }

    fn sched(
        &self,
        op: u32,
        pid: i32,
        policy: i32,
        param: [i32; 4],
    ) -> Result<(i32, [i32; 4]), Errno> {
        self.log(Request::Sched(SchedRequest {
            op,
            pid,
            policy,
            param,
        }));
        if let Some(errno) = self.take_failure(Op::Sched) {
            return Err(errno);
        }
        Ok((policy, param))
    }

    fn set_title(&self, title: &[u8]) -> Result<(), Errno> {
        self.log(Request::SetTitle(SetTitleRequest {
            title: title.to_vec(),
        }));
        self.take_failure(Op::SetTitle).map_or(Ok(()), Err)
    }

    fn prctl(&self, option: u32, a: [u64; 4]) -> Result<i64, Errno> {
        self.log(Request::Prctl(PrctlRequest { option, a }));
        self.take_failure(Op::Prctl).map_or(Ok(0), Err)
    }

    fn login(&self, set: Option<&[u8]>) -> Result<Vec<u8>, Errno> {
        self.log(Request::Login(LoginRequest {
            op: u32::from(set.is_some()),
            name: set.unwrap_or(b"").to_vec(),
        }));
        if let Some(errno) = self.take_failure(Op::Login) {
            return Err(errno);
        }
        Ok(set.unwrap_or(b"").to_vec())
    }

    fn pledge(&self, promises: Option<&[u8]>, exec_promises: Option<&[u8]>) -> Result<(), Errno> {
        let flags = u32::from(promises.is_some()) | (u32::from(exec_promises.is_some()) << 1);
        self.log(Request::Pledge(PledgeRequest {
            promises: promises.unwrap_or(b"").to_vec(),
            execpromises: exec_promises.unwrap_or(b"").to_vec(),
            flags,
        }));
        self.take_failure(Op::Pledge).map_or(Ok(()), Err)
    }

    fn cap_enter(&self) -> Result<(), Errno> {
        self.log(Request::CapEnter);
        self.take_failure(Op::CapEnter).map_or(Ok(()), Err)
    }
}

#[cfg(test)]
#[path = "fake_ut.rs"]
mod tests;
