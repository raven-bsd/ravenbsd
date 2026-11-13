// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the deterministic fake `procd`.

use super::*;
use crate::backend::WaitResult;
use crate::consts::RLIMIT_NOFILE;

fn spawn_request() -> SpawnRequest {
    SpawnRequest {
        spawn_flags: 0,
        pgroup: 0,
        sigdefault: [0; 128],
        sigmask: [0; 128],
        ignored: [0; 128],
        sched_policy: 0,
        sched_prio: 0,
        umask: 0,
        tcpgrp_fd: -1,
        path: b"/bin/true".to_vec(),
        cwd: b"/".to_vec(),
        argv: alloc::vec![b"true".to_vec()],
        envp: Vec::new(),
        fdmap: Vec::new(),
    }
}

#[test]
fn pids_start_at_one_hundred() {
    let procd = FakeProcd::new();
    assert_eq!(procd.spawn(&spawn_request()), Ok(100));
    assert_eq!(procd.spawn(&spawn_request()), Ok(101));
}

#[test]
fn a_spawn_is_logged() {
    let procd = FakeProcd::new();
    procd.spawn(&spawn_request()).expect("spawn");
    assert_eq!(procd.requests().len(), 1);
    assert!(matches!(procd.last_request(), Some(Request::Spawn(_))));
}

#[test]
fn a_scripted_spawn_result_wins() {
    let procd = FakeProcd::new();
    procd.push_spawn_result(Err(crate::errors::ENOENT));
    assert_eq!(procd.spawn(&spawn_request()), Err(crate::errors::ENOENT));
}

#[test]
fn fail_next_fails_only_the_named_operation() {
    let procd = FakeProcd::new();
    procd.fail_next(Op::Spawn, crate::errors::EACCES);
    assert_eq!(procd.spawn(&spawn_request()), Err(crate::errors::EACCES));
    assert_eq!(procd.spawn(&spawn_request()), Ok(100));
}

#[test]
fn a_scripted_wait_result_is_returned() {
    let procd = FakeProcd::new();
    let result = WaitResult {
        pid: 7,
        status: 0x100,
        siginfo: [0; 128],
        rusage: [0; 144],
    };
    procd.push_wait_result(Ok(result));
    assert_eq!(procd.wait(1, 7, 0), Ok(result));
}

#[test]
fn wait_without_a_script_is_echild() {
    let procd = FakeProcd::new();
    assert_eq!(procd.wait(1, 7, 0), Err(ECHILD));
}

#[test]
fn the_log_keeps_call_order() {
    let procd = FakeProcd::new();
    procd.spawn(&spawn_request()).expect("spawn");
    procd.kill(100, 15, None).expect("kill");
    let log = procd.requests();
    assert!(matches!(log[0], Request::Spawn(_)));
    assert!(matches!(log[1], Request::Kill(_)));
}

#[test]
fn kill_records_the_queue_value() {
    let procd = FakeProcd::new();
    procd.kill(1, 2, Some(9)).expect("kill");
    match procd.last_request() {
        Some(Request::Kill(request)) => {
            assert_eq!(request.pid, 1);
            assert_eq!(request.sig, 2);
            assert_eq!(request.flags, 1);
            assert_eq!(request.value, Some(9));
        }
        other => panic!("unexpected {other:?}"),
    }
}

#[test]
fn rlimit_reads_the_local_defaults() {
    let procd = FakeProcd::new();
    assert_eq!(procd.rlimit(0, RLIMIT_NOFILE, None), Ok((1024, 32768)));
}

#[test]
fn exec_returns_enosys_by_default() {
    let procd = FakeProcd::new();
    let request = ExecRequest {
        exec_flags: 0,
        fd_handle: 0,
        path: b"/x".to_vec(),
        cwd: b"/".to_vec(),
        argv: Vec::new(),
        envp: Vec::new(),
        close_list: Vec::new(),
        fdmap: Vec::new(),
        sigmask: [0; 128],
        ignored: [0; 128],
    };
    assert_eq!(procd.exec(&request), ENOSYS);
}

#[test]
fn cap_enter_is_logged() {
    let procd = FakeProcd::new();
    procd.cap_enter().expect("cap_enter");
    assert!(matches!(procd.last_request(), Some(Request::CapEnter)));
}

#[test]
fn the_fake_is_object_safe() {
    let procd = FakeProcd::new();
    let object: &dyn ProcBackend = &procd;
    assert_eq!(object.getpid(), 1);
}
