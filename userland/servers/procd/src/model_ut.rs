// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the procd process table and protocol dispatch.

#![allow(clippy::indexing_slicing)] // fixed test vectors
#![allow(clippy::arithmetic_side_effects)] // test construction

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec;

use super::*;
use raven_proto_proc::{
    Body, GetPgidRequest, GetSidRequest, HelloRequest, KillRequest, Op, Request, Response,
    SetPgidRequest, SpawnRequest, WaitRequest,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct SpawnCall {
    path: String,
    argv: Vec<String>,
    env: Vec<String>,
}

struct FakeSys {
    spawns: Vec<SpawnCall>,
    infos: BTreeMap<i32, ProcInfo>,
    handles: BTreeMap<u32, i32>,
    fail_info: BTreeMap<i32, usize>,
    waits: VecDeque<Result<i32, Errno>>,
    terminations: Vec<(Handle, i32)>,
    setpgids: Vec<(i32, i32)>,
    closed: Vec<Handle>,
    next_pid: i32,
    next_handle: u32,
    server_pid: i32,
}

impl FakeSys {
    fn new() -> Self {
        let mut infos = BTreeMap::new();
        infos.insert(
            3,
            ProcInfo {
                pid: 3,
                ppid: 0,
                pgid: 3,
                sid: 3,
            },
        );
        Self {
            spawns: Vec::new(),
            infos,
            handles: BTreeMap::new(),
            fail_info: BTreeMap::new(),
            waits: VecDeque::new(),
            terminations: Vec::new(),
            setpgids: Vec::new(),
            closed: Vec::new(),
            next_pid: 100,
            next_handle: 10,
            server_pid: 3,
        }
    }

    fn add_child(&mut self, pid: i32, ppid: i32, pgid: i32, sid: i32) {
        self.infos.insert(
            pid,
            ProcInfo {
                pid,
                ppid,
                pgid,
                sid,
            },
        );
    }
}

impl ProcSys for FakeSys {
    fn spawn(&mut self, path: &str, argv: &[&str], env: &[&str]) -> Result<Handle, Errno> {
        self.spawns.push(SpawnCall {
            path: path.to_string(),
            argv: argv.iter().map(|value| value.to_string()).collect(),
            env: env.iter().map(|value| value.to_string()).collect(),
        });
        let pid = self.next_pid;
        self.next_pid += 1;
        let handle = Handle(self.next_handle);
        self.next_handle += 1;
        self.handles.insert(handle.0, pid);
        self.add_child(pid, self.server_pid, self.server_pid, self.server_pid);
        Ok(handle)
    }

    fn pid(&mut self, handle: Handle) -> Result<i32, Errno> {
        self.handles.get(&handle.0).copied().ok_or(Errno::EBADF)
    }

    fn info(&mut self, pid: i32) -> Result<ProcInfo, Errno> {
        if let Some(remaining) = self.fail_info.get_mut(&pid) {
            if *remaining > 0 {
                *remaining -= 1;
                return Err(Errno::ESRCH);
            }
            self.fail_info.remove(&pid);
        }
        self.infos.get(&pid).copied().ok_or(Errno::ESRCH)
    }

    fn wait(&mut self, _handle: Handle) -> Result<i32, Errno> {
        self.waits.pop_front().unwrap_or(Err(Errno::ECHILD))
    }

    fn terminate(&mut self, handle: Handle, code: i32) -> Result<(), Errno> {
        self.terminations.push((handle, code));
        Ok(())
    }

    fn setpgid(&mut self, pid: i32, pgid: i32) -> Result<(), Errno> {
        self.setpgids.push((pid, pgid));
        if let Some(info) = self.infos.get_mut(&pid) {
            info.pgid = pgid;
        }
        Ok(())
    }

    fn close(&mut self, handle: Handle) {
        self.closed.push(handle);
    }

    fn self_pid(&mut self) -> Result<i32, Errno> {
        Ok(self.server_pid)
    }
}

fn zeroes() -> [u8; 128] {
    [0u8; 128]
}

fn spawn_request() -> SpawnRequest {
    SpawnRequest {
        spawn_flags: 0,
        pgroup: 0,
        sigdefault: zeroes(),
        sigmask: zeroes(),
        ignored: zeroes(),
        sched_policy: 0,
        sched_prio: 0,
        umask: 0o022,
        tcpgrp_fd: -1,
        path: b"/bin/procd-child".to_vec(),
        cwd: b"/".to_vec(),
        argv: vec![b"procd-child".to_vec(), b"exec".to_vec()],
        envp: vec![b"TEST=value".to_vec()],
        fdmap: vec![],
    }
}

fn spawn(procd: &mut Procd<FakeSys>, uid: u32) -> i32 {
    match procd.dispatch(uid, Request::Spawn(spawn_request())) {
        Response {
            status: 0,
            value,
            body: Body::Empty,
        } => i32::try_from(value).expect("pid"),
        other => panic!("spawn failed: {other:?}"),
    }
}

fn wait(procd: &mut Procd<FakeSys>, uid: u32, pid: i32, options: u32) -> Response {
    procd.dispatch(
        uid,
        Request::Wait(WaitRequest {
            idtype: P_PID,
            id: i64::from(pid),
            options,
        }),
    )
}

#[test]
fn hello_reports_the_server_version_and_pid() {
    let mut procd = Procd::new(FakeSys::new(), 3);
    let response = procd.dispatch(1000, Request::Hello(HelloRequest { client_version: 1 }));
    assert_eq!(
        response,
        Response::body(
            0,
            Body::Hello {
                server_version: 1,
                pid: 3
            }
        )
    );
}

#[test]
fn spawn_records_argv_env_and_wait_reports_the_exit_status() {
    let mut sys = FakeSys::new();
    sys.waits.push_back(Ok(7));
    let mut procd = Procd::new(sys, 3);
    let pid = spawn(&mut procd, 1000);
    assert_eq!(pid, 100);
    assert_eq!(
        procd.children().get(&pid).expect("child").image,
        "/bin/procd-child"
    );
    assert_eq!(
        procd.sys().spawns,
        vec![SpawnCall {
            path: "/bin/procd-child".to_string(),
            argv: vec!["procd-child".to_string(), "exec".to_string()],
            env: vec!["TEST=value".to_string()],
        }]
    );
    assert_eq!(procd.children().get(&pid).expect("child").owner, 1000);

    let response = wait(&mut procd, 1000, pid, 0);
    assert_eq!(response.value, i64::from(pid));
    assert_eq!(
        response.body,
        Body::Wait {
            status: 7 << 8,
            siginfo: [0u8; 128],
            rusage: [0u8; 144],
        }
    );
    assert!(!procd.children().contains_key(&pid));
    assert_eq!(
        wait(&mut procd, 1000, pid, 0).status,
        -(Errno::ECHILD.0 as i32)
    );
}

#[test]
fn kill_maps_supported_signals_to_terminate_and_wait_reports_signaled() {
    let mut sys = FakeSys::new();
    sys.waits.push_back(Ok(143));
    let mut procd = Procd::new(sys, 3);
    let pid = spawn(&mut procd, 1000);
    let response = procd.dispatch(
        1000,
        Request::Kill(KillRequest {
            pid: i64::from(pid),
            sig: SIGTERM,
            flags: 0,
            value: None,
        }),
    );
    assert_eq!(response.status, 0);
    assert_eq!(
        procd.children().get(&pid).expect("child").status,
        Some(WaitStatus::signaled(SIGTERM))
    );

    let response = wait(&mut procd, 1000, pid, 0);
    assert_eq!(
        response.body,
        Body::Wait {
            status: SIGTERM as i32,
            siginfo: [0u8; 128],
            rusage: [0u8; 144],
        }
    );
    let _ = procd;
}

#[test]
fn group_and_identity_operations_use_the_kernel_values() {
    let mut procd = Procd::new(FakeSys::new(), 3);
    let pid = spawn(&mut procd, 1000);
    let response = procd.dispatch(1000, Request::SetPgid(SetPgidRequest { pid, pgid: 0 }));
    assert_eq!(response.status, 0);
    assert_eq!(
        procd
            .dispatch(1000, Request::GetPgid(GetPgidRequest { pid }))
            .value,
        i64::from(pid)
    );
    assert_eq!(
        procd
            .dispatch(1000, Request::GetSid(GetSidRequest { pid }))
            .value,
        3
    );
}

#[test]
fn owner_checks_reject_another_unprivileged_client() {
    let mut sys = FakeSys::new();
    sys.waits.push_back(Ok(0));
    let mut procd = Procd::new(sys, 3);
    let pid = spawn(&mut procd, 1000);
    assert_eq!(
        wait(&mut procd, 1001, pid, 0).status,
        -(Errno::EPERM.0 as i32)
    );
    assert_eq!(wait(&mut procd, 0, pid, 0).status, 0);
}

#[test]
fn unsupported_rows_are_honest_enosys() {
    let mut procd = Procd::new(FakeSys::new(), 3);
    assert_eq!(
        procd.dispatch(1000, Request::Other(Op::Fork)).status,
        -(Errno::ENOSYS.0 as i32)
    );
    assert_eq!(
        procd.dispatch(1000, Request::Other(Op::Exec)).status,
        -(Errno::ENOSYS.0 as i32)
    );
    assert_eq!(
        procd.dispatch(1000, Request::Other(Op::SetSid)).status,
        -(Errno::ENOSYS.0 as i32)
    );
    assert_eq!(
        procd.dispatch(1000, Request::Other(Op::SetCred)).status,
        -(Errno::ENOSYS.0 as i32)
    );
    let pid = spawn(&mut procd, 1000);
    assert_eq!(
        wait(&mut procd, 1000, pid, WNOHANG).status,
        -(Errno::ENOSYS.0 as i32)
    );
    assert_eq!(
        procd
            .dispatch(
                1000,
                Request::Kill(KillRequest {
                    pid: i64::from(pid),
                    sig: 19,
                    flags: 0,
                    value: None,
                }),
            )
            .status,
        -(Errno::ENOSYS.0 as i32)
    );
}

#[test]
fn malformed_spawn_and_unknown_children_are_rejected() {
    let mut procd = Procd::new(FakeSys::new(), 3);
    let mut request = spawn_request();
    request.path = b"relative".to_vec();
    assert_eq!(
        procd.dispatch(1000, Request::Spawn(request)).status,
        -(Errno::EINVAL.0 as i32)
    );

    let mut request = spawn_request();
    request.argv = vec![];
    assert_eq!(
        procd.dispatch(1000, Request::Spawn(request)).status,
        -(Errno::EINVAL.0 as i32)
    );

    let mut request = spawn_request();
    request.tcpgrp_fd = 0;
    assert_eq!(
        procd.dispatch(1000, Request::Spawn(request)).status,
        -(Errno::ENOSYS.0 as i32)
    );

    assert_eq!(
        wait(&mut procd, 1000, 999, 0).status,
        -(Errno::ECHILD.0 as i32)
    );
}

#[test]
fn a_fast_exit_uses_the_server_group_and_session_as_the_fallback() {
    let mut sys = FakeSys::new();
    sys.fail_info.insert(100, 1);
    let mut procd = Procd::new(sys, 3);
    let pid = spawn(&mut procd, 1000);
    let child = procd.children().get(&pid).expect("child");
    assert_eq!(child.pgid, 3);
    assert_eq!(child.sid, 3);
}

#[test]
fn setpgid_spawn_flag_sets_the_child_group() {
    let mut procd = Procd::new(FakeSys::new(), 3);
    let mut request = spawn_request();
    request.spawn_flags = POSIX_SPAWN_SETPGROUP;
    request.pgroup = 0;
    let response = procd.dispatch(1000, Request::Spawn(request));
    assert_eq!(response.status, 0);
    let pid = i32::try_from(response.value).expect("pid");
    assert_eq!(procd.sys().setpgids, vec![(pid, pid)]);
    assert_eq!(procd.children().get(&pid).expect("child").pgid, pid);
}
