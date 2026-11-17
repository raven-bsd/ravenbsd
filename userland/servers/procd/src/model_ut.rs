// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the procd process table and protocol dispatch.

#![allow(clippy::indexing_slicing)] // fixed test vectors
#![allow(clippy::arithmetic_side_effects)] // test construction

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::vec;

use super::*;
use raven_proto_proc::{
    Body, FdMapEntry, GetPgidRequest, GetSidRequest, HelloRequest, KillRequest, Op, Request,
    Response, SetPgidRequest, SpawnRequest, WaitRequest,
};

#[derive(Clone, Debug, PartialEq, Eq)]
struct SpawnCall {
    path: String,
    argv: Vec<String>,
    env: Vec<String>,
    fdmap: Vec<(i32, u32)>,
    sigmask: [u8; 128],
    sigdefault: [u8; 128],
    ignored: [u8; 128],
}

struct FakeSys {
    spawns: Vec<SpawnCall>,
    infos: BTreeMap<i32, ProcInfo>,
    handles: BTreeMap<u32, i32>,
    fail_info: BTreeMap<i32, usize>,
    wait_events: VecDeque<Result<WaitEvent, Errno>>,
    signals: Vec<(i32, u32)>,
    setpgids: Vec<(i32, i32)>,
    closed: Vec<Handle>,
    reject_handles: BTreeSet<u32>,
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
            wait_events: VecDeque::new(),
            signals: Vec::new(),
            setpgids: Vec::new(),
            closed: Vec::new(),
            reject_handles: BTreeSet::new(),
            next_pid: 100,
            next_handle: 10,
            server_pid: 3,
        }
    }

    #[allow(clippy::too_many_arguments)] // mirrors the ProcSys attribute method
    fn record(
        &mut self,
        path: &str,
        argv: &[&str],
        env: &[&str],
        fdmap: &[(i32, u32)],
        sigmask: &[u8; 128],
        sigdefault: &[u8; 128],
        ignored: &[u8; 128],
    ) -> Handle {
        self.spawns.push(SpawnCall {
            path: path.to_string(),
            argv: argv.iter().map(|value| value.to_string()).collect(),
            env: env.iter().map(|value| value.to_string()).collect(),
            fdmap: fdmap.to_vec(),
            sigmask: *sigmask,
            sigdefault: *sigdefault,
            ignored: *ignored,
        });
        let pid = self.next_pid;
        self.next_pid += 1;
        let handle = Handle(self.next_handle);
        self.next_handle += 1;
        self.handles.insert(handle.0, pid);
        self.infos.insert(
            pid,
            ProcInfo {
                pid,
                ppid: self.server_pid,
                pgid: self.server_pid,
                sid: self.server_pid,
            },
        );
        handle
    }
}

impl ProcSys for FakeSys {
    fn spawn(&mut self, path: &str, argv: &[&str], env: &[&str]) -> Result<Handle, Errno> {
        Ok(self.record(path, argv, env, &[], &zeroes(), &zeroes(), &zeroes()))
    }

    fn spawn_with_attributes(
        &mut self,
        path: &str,
        argv: &[&str],
        env: &[&str],
        fdmap: &[(i32, u32)],
        sigmask: &[u8; 128],
        sigdefault: &[u8; 128],
        ignored: &[u8; 128],
    ) -> Result<Handle, Errno> {
        Ok(self.record(path, argv, env, fdmap, sigmask, sigdefault, ignored))
    }

    fn validate_handle(&mut self, handle: Handle) -> Result<(), Errno> {
        if handle == Handle::NONE || self.reject_handles.contains(&handle.0) {
            Err(Errno::EBADF)
        } else {
            Ok(())
        }
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

    fn wait_ex(&mut self, _idtype: u32, _id: i32, _options: u32) -> Result<WaitEvent, Errno> {
        self.wait_events.pop_front().unwrap_or(Err(Errno::ECHILD))
    }

    fn signal(&mut self, target: i32, signo: u32) -> Result<(), Errno> {
        self.signals.push((target, signo));
        Ok(())
    }

    fn getpgid(&mut self, pid: i32) -> Result<i32, Errno> {
        self.info(pid).map(|info| info.pgid)
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
    sys.wait_events.push_back(Ok(WaitEvent {
        pid: 100,
        status: 7 << 8,
    }));
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
            fdmap: vec![],
            sigmask: zeroes(),
            sigdefault: zeroes(),
            ignored: zeroes(),
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
fn wait_nohang_and_stop_continue_keep_the_child_alive() {
    let mut sys = FakeSys::new();
    sys.wait_events
        .push_back(Ok(WaitEvent { pid: 0, status: 0 }));
    let stop_status = ((SIGTERM & 0x7f) << 8) | 0x7f;
    sys.wait_events.push_back(Ok(WaitEvent {
        pid: 100,
        status: stop_status as i32,
    }));
    sys.wait_events.push_back(Ok(WaitEvent {
        pid: 100,
        status: 0xffff,
    }));
    sys.wait_events.push_back(Ok(WaitEvent {
        pid: 100,
        status: 0,
    }));
    let mut procd = Procd::new(sys, 3);
    let pid = spawn(&mut procd, 1000);
    let nohang = wait(&mut procd, 1000, pid, WNOHANG);
    assert_eq!(nohang.value, 0);
    let stopped = wait(&mut procd, 1000, pid, WUNTRACED);
    assert_eq!(
        stopped.body,
        Body::Wait {
            status: stop_status as i32,
            siginfo: [0u8; 128],
            rusage: [0u8; 144],
        }
    );
    assert!(procd.children().contains_key(&pid));
    let continued = wait(&mut procd, 1000, pid, WCONTINUED);
    assert_eq!(
        continued.body,
        Body::Wait {
            status: 0xffff,
            siginfo: [0u8; 128],
            rusage: [0u8; 144],
        }
    );
    assert!(procd.children().contains_key(&pid));
    let exited = wait(&mut procd, 1000, pid, 0);
    assert_eq!(
        exited.body,
        Body::Wait {
            status: 0,
            siginfo: [0u8; 128],
            rusage: [0u8; 144],
        }
    );
    assert!(!procd.children().contains_key(&pid));
}

#[test]
fn kill_uses_the_kernel_signal_path_and_wait_reports_the_exit_code() {
    let mut sys = FakeSys::new();
    sys.wait_events.push_back(Ok(WaitEvent {
        pid: 100,
        status: 143 << 8,
    }));
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
    assert_eq!(procd.sys().signals, vec![(pid, SIGTERM)]);
    let response = wait(&mut procd, 1000, pid, 0);
    assert_eq!(
        response.body,
        Body::Wait {
            status: 143 << 8,
            siginfo: [0u8; 128],
            rusage: [0u8; 144],
        }
    );
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
    let response = procd.dispatch(
        1000,
        Request::Kill(KillRequest {
            pid: -i64::from(pid),
            sig: SIGTERM,
            flags: 0,
            value: None,
        }),
    );
    assert_eq!(response.status, 0);
    assert_eq!(procd.sys().signals, vec![(-pid, SIGTERM)]);
}

#[test]
fn owner_checks_reject_another_unprivileged_client() {
    let mut sys = FakeSys::new();
    sys.wait_events.push_back(Ok(WaitEvent {
        pid: 100,
        status: 0,
    }));
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
    for op in [
        Op::Fork,
        Op::Exec,
        Op::SetSid,
        Op::SetCred,
        Op::Rlimit,
        Op::Rusage,
        Op::Prio,
        Op::Sched,
        Op::SessionInfo,
    ] {
        assert_eq!(
            procd.dispatch(1000, Request::Other(op)).status,
            -(Errno::ENOSYS.0 as i32),
            "{op:?}"
        );
    }
    let pid = spawn(&mut procd, 1000);
    assert_eq!(
        procd
            .dispatch(
                1000,
                Request::Kill(KillRequest {
                    pid: i64::from(pid),
                    sig: SIGTERM,
                    flags: 1,
                    value: None,
                }),
            )
            .status,
        -(Errno::ENOSYS.0 as i32)
    );
    assert_eq!(
        procd
            .dispatch(
                1000,
                Request::Wait(WaitRequest {
                    idtype: 99,
                    id: 0,
                    options: 0,
                }),
            )
            .status,
        -(Errno::EINVAL.0 as i32)
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

fn signal_bit(set: &mut [u8; 128], signo: u32) {
    let bit = usize::try_from(signo.saturating_sub(1)).expect("signal");
    set[bit / 8] |= 1 << (bit % 8);
}

fn fd(fd: i32, handle: u32) -> FdMapEntry {
    FdMapEntry { fd, handle }
}

#[test]
fn fdmap_and_signal_attributes_reach_the_loader_and_sources_close() {
    let mut sys = FakeSys::new();
    sys.reject_handles.insert(0xdead);
    let mut procd = Procd::new(sys, 3);
    let mut request = spawn_request();
    request.spawn_flags =
        POSIX_SPAWN_SETPGROUP | POSIX_SPAWN_SETSIGMASK | POSIX_SPAWN_SETSIGDEF | S_CLOEXEC_DEFAULT;
    request.pgroup = 0;
    request.fdmap = vec![fd(7, 0x40), fd(0, 0x41)];
    signal_bit(&mut request.sigmask, 10);
    signal_bit(&mut request.sigdefault, 15);
    signal_bit(&mut request.ignored, 12);
    let response = procd.dispatch(1000, Request::Spawn(request));
    assert_eq!(response.status, 0);
    let pid = i32::try_from(response.value).expect("pid");

    let call = procd.sys().spawns.first().expect("spawn");
    assert_eq!(call.fdmap, vec![(7, 0x40), (0, 0x41)]);
    assert_eq!(call.sigmask[1] & 0b10, 0b10);
    assert_eq!(call.sigdefault[1] & 0b0100_0000, 0b0100_0000);
    assert_eq!(call.ignored[1] & 0b1000, 0b1000);
    assert_eq!(procd.sys().closed, vec![Handle(0x40), Handle(0x41)]);

    let child = procd.children().get(&pid).expect("child");
    assert_eq!(child.fdmap, vec![(7, 0x40), (0, 0x41)]);
    assert_eq!(child.sigmask, call.sigmask);
    assert_eq!(child.sigdefault, call.sigdefault);
    assert_eq!(child.ignored, call.ignored);
}

#[test]
fn malformed_fd_maps_are_rejected_before_spawning() {
    let cases: Vec<(Vec<FdMapEntry>, i32)> = vec![
        (vec![fd(7, 0x40), fd(7, 0x41)], Errno::EINVAL.0 as i32),
        (vec![fd(-1, 0x40)], Errno::EINVAL.0 as i32),
        (vec![fd(MAX_CHILD_FD, 0x40)], Errno::EINVAL.0 as i32),
        (vec![fd(7, Handle::NONE.0)], Errno::EINVAL.0 as i32),
        (vec![fd(7, 0xdead)], Errno::EBADF.0 as i32),
    ];
    for (map, expected) in cases {
        let mut sys = FakeSys::new();
        sys.reject_handles.insert(0xdead);
        let mut procd = Procd::new(sys, 3);
        let mut request = spawn_request();
        request.fdmap = map;
        let response = procd.dispatch(1000, Request::Spawn(request));
        assert_eq!(response.status, -expected);
        assert!(procd.sys().spawns.is_empty());
        assert!(!procd.sys().closed.is_empty());
    }
}

#[test]
fn too_many_fd_map_entries_are_rejected() {
    let mut procd = Procd::new(FakeSys::new(), 3);
    let mut request = spawn_request();
    request.fdmap = (0..=raven_proto_proc::MAX_FDS)
        .map(|index| fd(i32::try_from(index).expect("fd"), 0x1000))
        .collect();
    let response = procd.dispatch(1000, Request::Spawn(request));
    assert_eq!(response.status, -(Errno::EINVAL.0 as i32));
    assert!(procd.sys().spawns.is_empty());
}

#[test]
fn invalid_signal_sets_are_rejected_before_spawning() {
    let cases = [
        {
            let mut request = spawn_request();
            request.spawn_flags = POSIX_SPAWN_SETSIGMASK;
            signal_bit(&mut request.sigmask, SIGKILL);
            request
        },
        {
            let mut request = spawn_request();
            signal_bit(&mut request.ignored, SIGSTOP);
            request
        },
        {
            let mut request = spawn_request();
            request.spawn_flags = POSIX_SPAWN_SETSIGMASK;
            request.sigmask[8] = 1;
            request
        },
        {
            let mut request = spawn_request();
            signal_bit(&mut request.sigdefault, 15);
            request
        },
    ];
    for request in cases {
        let mut procd = Procd::new(FakeSys::new(), 3);
        let response = procd.dispatch(1000, Request::Spawn(request));
        assert_eq!(response.status, -(Errno::EINVAL.0 as i32));
        assert!(procd.sys().spawns.is_empty());
    }
}

#[test]
fn unsupported_spawn_flags_still_return_enosys() {
    let mutations: [fn(&mut SpawnRequest); 3] = [
        |request| request.spawn_flags = POSIX_SPAWN_SETSID,
        |request| request.sched_policy = 1,
        |request| request.tcpgrp_fd = 0,
    ];
    for mutate in mutations {
        let mut procd = Procd::new(FakeSys::new(), 3);
        let mut request = spawn_request();
        mutate(&mut request);
        request.fdmap = vec![fd(7, 0x40)];
        let response = procd.dispatch(1000, Request::Spawn(request));
        assert_eq!(response.status, -(Errno::ENOSYS.0 as i32));
        assert!(procd.sys().spawns.is_empty());
        assert_eq!(procd.sys().closed, vec![Handle(0x40)]);
    }
}

#[test]
fn negative_pgroup_is_rejected_before_spawning() {
    let mut procd = Procd::new(FakeSys::new(), 3);
    let mut request = spawn_request();
    request.spawn_flags = POSIX_SPAWN_SETPGROUP;
    request.pgroup = -5;
    let response = procd.dispatch(1000, Request::Spawn(request));
    assert_eq!(response.status, -(Errno::EINVAL.0 as i32));
    assert!(procd.sys().spawns.is_empty());
}

#[test]
fn spawn_records_argv_zero_as_the_command() {
    let mut procd = Procd::new(FakeSys::new(), 3);
    let pid = spawn(&mut procd, 1000);
    let child = procd.children().get(&pid).expect("child");
    assert_eq!(child.command, "procd-child");
    assert_eq!(
        procd.registry()[0].to_string(),
        format!("{pid} 3 3 3 1000 procd-child")
    );
}

#[test]
fn registry_lists_children_by_pid_and_drops_them_after_wait() {
    let mut sys = FakeSys::new();
    sys.wait_events.push_back(Ok(WaitEvent {
        pid: 100,
        status: 0,
    }));
    let mut procd = Procd::new(sys, 3);
    let pid = spawn(&mut procd, 1000);
    let entries = procd.registry();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].pid, pid);
    assert_eq!(entries[0].ppid, 3);
    assert_eq!(entries[0].uid, 1000);
    assert_eq!(entries[0].command, "procd-child");
    assert_eq!(wait(&mut procd, 1000, pid, 0).status, 0);
    assert!(procd.registry().is_empty());
}

#[test]
fn command_names_are_argv_zero_truncated_at_utf8_boundaries() {
    assert_eq!(command_name(&["shell", "-c"], "/bin/sh"), "shell");
    assert_eq!(command_name(&[], "/bin/sh"), "/bin/sh");
    let long = "é".repeat(MAX_COMMAND);
    let truncated = command_name(&[long.as_str()], "/bin/sh");
    assert!(truncated.len() <= MAX_COMMAND);
    assert!(truncated.is_char_boundary(truncated.len()));
}
