// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the early-boot backend.

use super::*;
use crate::backend::FdMapEntry;
use crate::consts::RLIMIT_NOFILE;

struct Spawner {
    calls: core::cell::Cell<usize>,
    path: core::cell::RefCell<Vec<u8>>,
}

impl Spawner {
    fn new() -> Self {
        Self {
            calls: core::cell::Cell::new(0),
            path: core::cell::RefCell::new(Vec::new()),
        }
    }
}

impl EarlySpawn for Spawner {
    fn spawn_initfs(
        &self,
        path: &[u8],
        _argv: &[Vec<u8>],
        _envp: &[Vec<u8>],
        _fds: &[FdMapEntry],
    ) -> Result<i32, Errno> {
        self.calls.set(self.calls.get() + 1);
        *self.path.borrow_mut() = path.to_vec();
        Ok(55)
    }
}

fn spawn_request(path: &[u8]) -> crate::wire::SpawnRequest {
    crate::wire::SpawnRequest {
        spawn_flags: 0,
        pgroup: 0,
        sigdefault: [0; 128],
        sigmask: [0; 128],
        ignored: [0; 128],
        sched_policy: 0,
        sched_prio: 0,
        umask: 0,
        tcpgrp_fd: -1,
        path: path.to_vec(),
        cwd: b"/".to_vec(),
        argv: Vec::new(),
        envp: Vec::new(),
        fdmap: Vec::new(),
    }
}

#[test]
fn identity_is_fixed() {
    let backend = LocalBackend::new(None);
    assert_eq!(backend.getpid(), 1);
    assert_eq!(backend.getppid(), 0);
    assert_eq!(backend.getuid(), 0);
    assert_eq!(backend.geteuid(), 0);
    assert_eq!(backend.getgid(), 0);
    assert_eq!(backend.getegid(), 0);
}

#[test]
fn getpgid_and_getsid() {
    let backend = LocalBackend::new(None);
    assert_eq!(backend.getpgid(0), Ok(1));
    assert_eq!(backend.getpgid(1), Ok(1));
    assert_eq!(backend.getpgid(2), Err(ESRCH));
    assert_eq!(backend.getsid(0), Ok(1));
    assert_eq!(backend.getsid(1), Ok(1));
    assert_eq!(backend.getsid(2), Err(ESRCH));
}

#[test]
fn getgroups_is_empty() {
    let backend = LocalBackend::new(None);
    let mut groups = [7u32; 4];
    assert_eq!(backend.getgroups(&mut groups), Ok(0));
}

#[test]
fn spawn_without_an_early_spawner_is_enosys() {
    let backend = LocalBackend::new(None);
    assert_eq!(backend.spawn(&spawn_request(b"/init")), Err(ENOSYS));
}

#[test]
fn spawn_with_an_early_spawner_forwards() {
    let spawner = Spawner::new();
    let backend = LocalBackend::new(Some(&spawner));
    assert_eq!(backend.spawn(&spawn_request(b"/init")), Ok(55));
    assert_eq!(spawner.calls.get(), 1);
    assert_eq!(*spawner.path.borrow(), b"/init");
}

#[test]
fn fork_and_exec_are_enosys() {
    let backend = LocalBackend::new(None);
    assert_eq!(
        backend.fork(&crate::wire::ForkRequest {
            entry: 0,
            sp: 0,
            arg: 0,
            fork_flags: 0
        }),
        Err(ENOSYS)
    );
    assert_eq!(
        backend.exec(&crate::wire::ExecRequest {
            exec_flags: 0,
            fd_handle: 0,
            path: Vec::new(),
            cwd: Vec::new(),
            argv: Vec::new(),
            envp: Vec::new(),
            close_list: Vec::new(),
            fdmap: Vec::new(),
            sigmask: [0; 128],
            ignored: [0; 128]
        }),
        ENOSYS
    );
}

#[test]
fn wait_is_echild() {
    let backend = LocalBackend::new(None);
    assert_eq!(backend.wait(0, 0, 0), Err(ECHILD));
}

#[test]
fn kill_is_esrch() {
    let backend = LocalBackend::new(None);
    assert_eq!(backend.kill(7, 15, None), Err(ESRCH));
}

#[test]
fn process_mutations_are_enosys() {
    let backend = LocalBackend::new(None);
    assert_eq!(backend.setpgid(1, 2), Err(ENOSYS));
    assert_eq!(backend.setsid(), Err(ENOSYS));
    assert_eq!(backend.set_title(b"x"), Err(ENOSYS));
    assert_eq!(backend.prctl(1, [0; 4]), Err(ENOSYS));
    assert_eq!(backend.login(None), Err(ENOSYS));
    assert_eq!(backend.pledge(None, None), Err(ENOSYS));
    assert_eq!(backend.cap_enter(), Err(ENOSYS));
    assert_eq!(backend.sched(0, 0, 0, [0; 4]), Err(ENOSYS));
}

#[test]
fn setcred_is_enosys() {
    let backend = LocalBackend::new(None);
    assert_eq!(
        backend.setcred(&crate::ids::SetCred::setuid(1)),
        Err(ENOSYS)
    );
}

#[test]
fn rlimit_reads_the_defaults() {
    let backend = LocalBackend::new(None);
    assert_eq!(backend.rlimit(0, RLIMIT_NOFILE, None), Ok((1024, 32768)));
}

#[test]
fn rlimit_writes_are_enosys() {
    let backend = LocalBackend::new(None);
    assert_eq!(backend.rlimit(0, RLIMIT_NOFILE, Some((1, 2))), Err(ENOSYS));
}

#[test]
fn rusage_is_zeroed() {
    let backend = LocalBackend::new(None);
    assert_eq!(backend.rusage(0), Ok([0; 144]));
}

#[test]
fn prio_get_is_zero_and_set_is_enosys() {
    let backend = LocalBackend::new(None);
    assert_eq!(backend.prio(0, 0, None), Ok(0));
    assert_eq!(backend.prio(0, 0, Some(5)), Err(ENOSYS));
}
