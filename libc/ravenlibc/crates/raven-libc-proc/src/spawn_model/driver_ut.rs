// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the `posix_spawn` driver.

use super::*;
use crate::backend::{ProcBackend, WaitResult};
use crate::errors::{EACCES, ENOEXEC};
use crate::ids::SetCred;
use crate::spawn_model::api::{FileAction, FileActions, SpawnAttr};
use alloc::vec;
use alloc::vec::Vec;
use core::cell::RefCell;

struct DriverEnv {
    opens: Vec<u32>,
    closed: Vec<u32>,
}

impl DriverEnv {
    fn new(opens: Vec<u32>) -> Self {
        Self {
            opens,
            closed: Vec::new(),
        }
    }
}

impl SpawnEnv for DriverEnv {
    fn open(&mut self, _path: &[u8], _oflag: i32, _mode: u32) -> Result<u32, Errno> {
        if self.opens.is_empty() {
            return Err(ENOSYS);
        }
        Ok(self.opens.remove(0))
    }

    fn is_dir(&mut self, _handle: u32) -> Result<bool, Errno> {
        Ok(true)
    }

    fn close_temp(&mut self, handle: u32) {
        self.closed.push(handle);
    }

    fn fd_path(&mut self, _handle: u32) -> Option<Vec<u8>> {
        Some(b"/".to_vec())
    }
}

struct FakeBackend {
    requests: RefCell<Vec<SpawnRequest>>,
    spawn_results: RefCell<Vec<Result<i32, Errno>>>,
}

impl FakeBackend {
    fn new() -> Self {
        Self {
            requests: RefCell::new(Vec::new()),
            spawn_results: RefCell::new(Vec::new()),
        }
    }

    fn fail_next(&self, error: Errno) {
        self.spawn_results.borrow_mut().push(Err(error));
    }

    fn push_spawn_result(&self, result: Result<i32, Errno>) {
        self.spawn_results.borrow_mut().push(result);
    }

    fn last_request(&self) -> Option<SpawnRequest> {
        self.requests.borrow().last().cloned()
    }

    fn requests(&self) -> Vec<SpawnRequest> {
        self.requests.borrow().clone()
    }
}

impl ProcBackend for FakeBackend {
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

    fn getpgid(&self, _pid: i32) -> Result<i32, Errno> {
        Ok(1)
    }

    fn getsid(&self, _pid: i32) -> Result<i32, Errno> {
        Ok(1)
    }

    fn getgroups(&self, _out: &mut [u32]) -> Result<usize, Errno> {
        Ok(0)
    }

    fn spawn(&self, req: &SpawnRequest) -> Result<i32, Errno> {
        let pid = 100 + i32::try_from(self.requests.borrow().len()).unwrap_or(0);
        self.requests.borrow_mut().push(req.clone());
        let mut results = self.spawn_results.borrow_mut();
        if results.is_empty() {
            Ok(pid)
        } else {
            results.remove(0)
        }
    }

    fn fork(&self, _req: &crate::request::ForkRequest) -> Result<i32, Errno> {
        Err(ENOSYS)
    }

    fn exec(&self, _req: &crate::request::ExecRequest) -> Errno {
        ENOSYS
    }

    fn wait(&self, _idtype: u32, _id: i64, _options: u32) -> Result<WaitResult, Errno> {
        Err(crate::errors::ECHILD)
    }

    fn kill(&self, _pid: i64, _sig: u32, _queue_value: Option<u64>) -> Result<(), Errno> {
        Err(crate::errors::ESRCH)
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

fn ctx<'a>(table: &'a [FdEntry], cwd: &'a [u8]) -> SpawnCtx<'a> {
    SpawnCtx {
        table,
        cwd,
        sigmask: [3; 128],
        ignored: [4; 128],
        umask: 0o022,
    }
}

fn spawn_request(backend: &FakeBackend) -> SpawnRequest {
    backend.last_request().expect("spawn request")
}

#[test]
fn success_returns_zero_and_the_child_pid() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &backend,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            None,
            None,
            &[b"true".to_vec()],
            &[]
        ),
        0
    );
    assert_eq!(
        spawn_model_pid(
            &backend,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            None,
            None,
            &[b"true".to_vec()],
            &[]
        ),
        Ok(101)
    );
}

#[test]
fn a_backend_error_is_returned_as_the_value() {
    let backend = FakeBackend::new();
    backend.fail_next(EACCES);
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &backend,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            None,
            None,
            &[b"true".to_vec()],
            &[]
        ),
        EACCES
    );
}

#[test]
fn the_request_carries_the_inherited_fdmap() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(Vec::new());
    let table = [FdEntry {
        fd: 1,
        handle: 7,
        kind: 0,
        status: 2,
        path: None,
        event: 0,
        readiness: 0,
        socket: None,
        pty_master: false,
        cloexec: false,
    }];
    let context = ctx(&table, b"/tmp");
    posix_spawn_model(
        &backend,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        None,
        None,
        &[b"true".to_vec()],
        &[],
    );
    let request = spawn_request(&backend);
    assert_eq!(
        request.fdmap,
        vec![crate::request::FdMapEntry {
            fd: 1,
            handle: 7,
            kind: 0,
            status: 2,
            path: None,
            event: 0,
            readiness: 0,
            socket: None,
            pty_master: false,
        }]
    );
    assert_eq!(request.cwd, b"/tmp".to_vec());
    assert_eq!(request.umask, 0o022);
    assert_eq!(request.sigmask, [3; 128]);
    assert_eq!(request.ignored, [4; 128]);
}

#[test]
fn temporary_handles_are_closed_after_success() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(vec![9]);
    let mut actions = FileActions::new();
    actions.add_open(3, b"/dev/null", 0, 0).expect("open");
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &backend,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        Some(&actions),
        None,
        &[b"true".to_vec()],
        &[],
    );
    assert_eq!(env.closed, vec![9]);
    assert_eq!(
        spawn_request(&backend).fdmap,
        vec![crate::request::FdMapEntry {
            fd: 3,
            handle: 9,
            kind: 0,
            status: 0,
            path: None,
            event: 0,
            readiness: 0,
            socket: None,
            pty_master: false,
        }]
    );
}

#[test]
fn temporary_handles_are_closed_after_failure() {
    let backend = FakeBackend::new();
    backend.fail_next(EACCES);
    let mut env = DriverEnv::new(vec![9]);
    let mut actions = FileActions::new();
    actions.add_open(3, b"/dev/null", 0, 0).expect("open");
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &backend,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            Some(&actions),
            None,
            &[b"true".to_vec()],
            &[]
        ),
        EACCES
    );
    assert_eq!(env.closed, vec![9]);
}

#[test]
fn an_empty_path_is_enoent_without_a_request() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &backend,
            &mut env,
            &context,
            b"",
            false,
            None,
            None,
            None,
            &[],
            &[]
        ),
        ENOENT
    );
    assert!(backend.requests().is_empty());
}

#[test]
fn too_many_arguments_are_rejected_before_a_request() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    let argv = vec![b"x".to_vec(); MAX_ARGS + 1];
    assert_eq!(
        posix_spawn_model(
            &backend,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            None,
            None,
            &argv,
            &[]
        ),
        E2BIG
    );
    assert!(backend.requests().is_empty());
}

#[test]
fn too_many_argument_bytes_are_rejected_before_a_request() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    let argv = vec![vec![b'x'; ARG_MAX]];
    assert_eq!(
        posix_spawn_model(
            &backend,
            &mut env,
            &context,
            b"/bin/true",
            false,
            None,
            None,
            None,
            &argv,
            &[]
        ),
        E2BIG
    );
    assert!(backend.requests().is_empty());
}

#[test]
fn a_path_search_tries_each_element() {
    let backend = FakeBackend::new();
    backend.push_spawn_result(Err(ENOENT));
    backend.push_spawn_result(Ok(100));
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &backend,
            &mut env,
            &context,
            b"tool",
            true,
            Some(b"/a:/b"),
            None,
            None,
            &[b"tool".to_vec()],
            &[]
        ),
        0
    );
    let requests = backend.requests();
    let paths: Vec<Vec<u8>> = requests.iter().map(|spawn| spawn.path.clone()).collect();
    assert_eq!(paths, vec![b"/a/tool".to_vec(), b"/b/tool".to_vec()]);
}

#[test]
fn enoexec_retries_with_the_shell() {
    let backend = FakeBackend::new();
    backend.push_spawn_result(Err(ENOEXEC));
    backend.push_spawn_result(Ok(100));
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    assert_eq!(
        posix_spawn_model(
            &backend,
            &mut env,
            &context,
            b"script",
            true,
            Some(b"/a"),
            None,
            None,
            &[b"script".to_vec(), b"one".to_vec()],
            &[]
        ),
        0
    );
    let requests = backend.requests();
    assert_eq!(requests.len(), 2);
    let spawn = &requests[1];
    assert_eq!(spawn.path, b"/bin/sh".to_vec());
    assert_eq!(
        spawn.argv,
        vec![b"sh".to_vec(), b"/a/script".to_vec(), b"one".to_vec()]
    );
}

#[test]
fn a_slash_name_is_a_direct_spawn() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &backend,
        &mut env,
        &context,
        b"/bin/tool",
        true,
        Some(b"/a"),
        None,
        None,
        &[b"tool".to_vec()],
        &[],
    );
    assert_eq!(backend.requests().len(), 1);
    assert_eq!(spawn_request(&backend).path, b"/bin/tool".to_vec());
}

#[test]
fn attributes_are_forwarded() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(Vec::new());
    let mut attr = SpawnAttr::new();
    attr.set_flags(
        crate::consts::POSIX_SPAWN_SETPGROUP
            | crate::consts::POSIX_SPAWN_SETSIGMASK
            | crate::consts::POSIX_SPAWN_SETSIGDEF
            | crate::consts::POSIX_SPAWN_SETSCHEDULER,
    )
    .expect("flags");
    attr.set_pgroup(7);
    attr.set_sigmask([1; 128]);
    attr.set_sigdefault([2; 128]);
    attr.set_sched_policy(1);
    attr.set_sched_priority(9);
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &backend,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        None,
        Some(&attr),
        &[b"true".to_vec()],
        &[],
    );
    let request = spawn_request(&backend);
    assert_eq!(request.sigmask, [1; 128]);
    assert_eq!(request.sigdefault, [2; 128]);
    assert_eq!(request.pgroup, 7);
    assert_eq!(request.sched_policy, 1);
    assert_eq!(request.sched_prio, 9);
}

#[test]
fn cloexec_default_sets_the_hint_and_hides_inherited_fds() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(Vec::new());
    let mut attr = SpawnAttr::new();
    attr.set_flags(POSIX_SPAWN_CLOEXEC_DEFAULT).expect("flags");
    let table = [FdEntry {
        fd: 1,
        handle: 7,
        kind: 0,
        status: 2,
        path: None,
        event: 0,
        readiness: 0,
        socket: None,
        pty_master: false,
        cloexec: false,
    }];
    let context = ctx(&table, b"/");
    posix_spawn_model(
        &backend,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        None,
        Some(&attr),
        &[b"true".to_vec()],
        &[],
    );
    let request = spawn_request(&backend);
    assert_eq!(request.spawn_flags & S_CLOEXEC_DEFAULT, S_CLOEXEC_DEFAULT);
    assert!(request.fdmap.is_empty());
}

#[test]
fn a_path_search_sets_the_path_hint() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(Vec::new());
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &backend,
        &mut env,
        &context,
        b"true",
        true,
        Some(b"/bin"),
        None,
        None,
        &[b"true".to_vec()],
        &[],
    );
    assert_eq!(
        spawn_request(&backend).spawn_flags & S_USE_PATH_HINT,
        S_USE_PATH_HINT
    );
}

#[test]
fn an_open_action_is_in_the_request_and_closed() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(vec![9]);
    let mut actions = FileActions::new();
    actions.add_open(3, b"/dev/null", 0, 0).expect("open");
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &backend,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        Some(&actions),
        None,
        &[b"true".to_vec()],
        &[],
    );
    assert!(matches!(actions.actions()[0], FileAction::Open { .. }));
    assert_eq!(env.closed, vec![9]);
    assert_eq!(
        spawn_request(&backend).fdmap,
        vec![crate::request::FdMapEntry {
            fd: 3,
            handle: 9,
            kind: 0,
            status: 0,
            path: None,
            event: 0,
            readiness: 0,
            socket: None,
            pty_master: false,
        }]
    );
}

#[test]
fn spawn_with_mask_sets_both_signal_flags() {
    let backend = FakeBackend::new();
    let context = ctx(&[], b"/");
    assert_eq!(
        spawn_with_mask(
            &backend,
            &context,
            b"/bin/sh",
            &[b"sh".to_vec()],
            &[],
            [5; 128],
            [6; 128]
        ),
        Ok(100)
    );
    let request = spawn_request(&backend);
    assert_eq!(
        request.spawn_flags & 0x000c,
        (POSIX_SPAWN_SETSIGMASK | POSIX_SPAWN_SETSIGDEF) as u32
    );
    assert_eq!(request.sigmask, [5; 128]);
    assert_eq!(request.sigdefault, [6; 128]);
}

#[test]
fn tcsetpgrp_is_forwarded() {
    let backend = FakeBackend::new();
    let mut env = DriverEnv::new(Vec::new());
    let mut actions = FileActions::new();
    actions.add_tcsetpgrp(2).expect("tcsetpgrp");
    let context = ctx(&[], b"/");
    posix_spawn_model(
        &backend,
        &mut env,
        &context,
        b"/bin/true",
        false,
        None,
        Some(&actions),
        None,
        &[b"true".to_vec()],
        &[],
    );
    assert_eq!(spawn_request(&backend).tcpgrp_fd, 2);
}
