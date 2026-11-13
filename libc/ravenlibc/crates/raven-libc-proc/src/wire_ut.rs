// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the procd wire encoder and response decoder.

#![allow(clippy::indexing_slicing)] // fixed wire buffers
#![allow(clippy::arithmetic_side_effects)] // test vector construction

use super::*;
use alloc::vec;

fn zeros() -> [u8; 128] {
    [0u8; 128]
}

fn spawn() -> Request {
    Request::Spawn(SpawnRequest {
        spawn_flags: 0,
        pgroup: 0,
        sigdefault: zeros(),
        sigmask: zeros(),
        ignored: zeros(),
        sched_policy: 0,
        sched_prio: 0,
        umask: 0o022,
        tcpgrp_fd: -1,
        path: b"/bin/true".to_vec(),
        cwd: b"/".to_vec(),
        argv: vec![b"true".to_vec()],
        envp: vec![],
        fdmap: vec![FdMapEntry { fd: 0, handle: 1 }],
    })
}

fn fork() -> Request {
    Request::Fork(ForkRequest {
        entry: 0x1000,
        sp: 0x2000,
        arg: 0x3000,
        fork_flags: 1,
    })
}

fn exec() -> Request {
    Request::Exec(ExecRequest {
        exec_flags: 0,
        fd_handle: 0,
        path: b"/bin/true".to_vec(),
        cwd: b"/".to_vec(),
        argv: vec![b"true".to_vec()],
        envp: vec![],
        close_list: vec![3],
        fdmap: vec![FdMapEntry { fd: 0, handle: 1 }],
        sigmask: zeros(),
        ignored: zeros(),
    })
}

fn wait() -> Request {
    Request::Wait(WaitRequest {
        idtype: 1,
        id: 42,
        options: 1,
    })
}

fn kill() -> Request {
    Request::Kill(KillRequest {
        pid: -1,
        sig: 15,
        flags: 0,
        value: None,
    })
}

fn setpgid() -> Request {
    Request::SetPgid(SetPgidRequest { pid: 1, pgid: 2 })
}

fn getpgid() -> Request {
    Request::GetPgid(GetPgidRequest { pid: 1 })
}

fn getsid() -> Request {
    Request::GetSid(GetSidRequest { pid: 1 })
}

fn setcred() -> Request {
    Request::SetCred(SetCredRequest {
        kind: 4,
        a: 1,
        b: 2,
        c: 3,
        groups: vec![4, 5],
    })
}

fn getgroups() -> Request {
    Request::GetGroups(GetGroupsRequest { max: 32 })
}

fn rlimit() -> Request {
    Request::Rlimit(RlimitRequest {
        pid: 1,
        resource: 7,
        has_new: 1,
        new_cur: 1024,
        new_max: 32768,
    })
}

fn rusage() -> Request {
    Request::Rusage(RusageRequest { who: 0 })
}

fn prio() -> Request {
    Request::Prio(PrioRequest {
        which: 0,
        who: 0,
        op: 1,
        value: 5,
    })
}

fn sched() -> Request {
    Request::Sched(SchedRequest {
        op: 1,
        pid: 1,
        policy: 0,
        param: [1, 2, 3, 4],
    })
}

fn title() -> Request {
    Request::SetTitle(SetTitleRequest {
        title: b"ps".to_vec(),
    })
}

fn prctl() -> Request {
    Request::Prctl(PrctlRequest {
        option: 1,
        a: [2, 3, 4, 5],
    })
}

fn login() -> Request {
    Request::Login(LoginRequest {
        op: 1,
        name: b"alice".to_vec(),
    })
}

fn pledge() -> Request {
    Request::Pledge(PledgeRequest {
        promises: b"stdio".to_vec(),
        execpromises: b"".to_vec(),
        flags: 1,
    })
}

fn session() -> Request {
    Request::SessionInfo(SessionInfoRequest { pid: 1 })
}

fn assert_request(request: &Request) {
    let length = measure(request).expect("measure");
    let mut buffer = vec![0u8; length];
    let encoded = encode(request, 7, &mut buffer).expect("encode");
    assert_eq!(encoded, length);
    assert_eq!(&buffer[0..2], &VERSION.to_le_bytes());
    assert_eq!(&buffer[2..4], &(request.op() as u16).to_le_bytes());
    assert_eq!(&buffer[4..8], &0u32.to_le_bytes());
    assert_eq!(&buffer[8..16], &7u64.to_le_bytes());
}

macro_rules! header_test {
    ($name:ident, $request:expr) => {
        #[test]
        fn $name() {
            assert_request(&$request);
        }
    };
}

header_test!(
    hello_header,
    Request::Hello(HelloRequest { client_version: 1 })
);
header_test!(spawn_header, spawn());
header_test!(fork_header, fork());
header_test!(exec_header, exec());
header_test!(wait_header, wait());
header_test!(kill_header, kill());
header_test!(setpgid_header, setpgid());
header_test!(setsid_header, Request::SetSid);
header_test!(getpgid_header, getpgid());
header_test!(getsid_header, getsid());
header_test!(setcred_header, setcred());
header_test!(getgroups_header, getgroups());
header_test!(rlimit_header, rlimit());
header_test!(rusage_header, rusage());
header_test!(prio_header, prio());
header_test!(sched_header, sched());
header_test!(title_header, title());
header_test!(prctl_header, prctl());
header_test!(login_header, login());
header_test!(pledge_header, pledge());
header_test!(capenter_header, Request::CapEnter);
header_test!(session_header, session());

#[test]
fn set_sid_golden() {
    let mut buffer = [0u8; 32];
    let length = encode(&Request::SetSid, 7, &mut buffer).expect("encode");
    assert_eq!(length, 16);
    assert_eq!(
        &buffer[..16],
        &[
            0x01, 0x00, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00,
        ]
    );
}

#[test]
fn kill_golden() {
    let mut buffer = [0u8; 64];
    let length = encode(&kill(), 1, &mut buffer).expect("encode");
    assert_eq!(length, 32);
    assert_eq!(
        &buffer[..32],
        &[
            0x01, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x0f, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00,
        ]
    );
}

#[test]
fn set_title_golden() {
    let mut buffer = [0u8; 32];
    let length = encode(&title(), 1, &mut buffer).expect("encode");
    assert_eq!(length, 24);
    assert_eq!(
        &buffer[..24],
        &[
            0x01, 0x00, 0x11, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x70, 0x73, 0x00, 0x00,
        ]
    );
}

#[test]
fn response_header_golden() {
    let bytes = [
        0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x2a, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
        0x00,
    ];
    let header = ResponseHeader::decode(&bytes).expect("header");
    assert_eq!(header.status, 0);
    assert_eq!(header.value, 42);
    assert_eq!(
        decode_response(Op::SetSid, &bytes),
        Ok((header, Body::Empty))
    );
}

#[test]
fn response_eperm_is_an_error() {
    let mut bytes = [0u8; 16];
    bytes[..4].copy_from_slice(&(-1i32).to_le_bytes());
    assert_eq!(
        decode_response(Op::SetSid, &bytes),
        Err(crate::errors::EPERM)
    );
}

#[test]
fn response_out_of_range_is_eproto() {
    let mut bytes = [0u8; 16];
    bytes[..4].copy_from_slice(&(-4096i32).to_le_bytes());
    assert_eq!(ResponseHeader::decode(&bytes), Err(EPROTO));
}

#[test]
fn a_short_header_is_einval() {
    assert_eq!(ResponseHeader::decode(&[0u8; 15]), Err(EINVAL));
}

#[test]
fn encode_rejects_a_short_buffer() {
    let mut buffer = [0u8; 8];
    assert_eq!(encode(&Request::SetSid, 1, &mut buffer), Err(E2BIG));
}

#[test]
fn oversized_strings_are_rejected() {
    let request = Request::SetTitle(SetTitleRequest {
        title: vec![b'x'; MAX_STRING + 1],
    });
    assert_eq!(measure(&request), Err(ENAMETOOLONG));
}

#[test]
fn too_many_arguments_are_rejected() {
    let request = Request::Spawn(SpawnRequest {
        spawn_flags: 0,
        pgroup: 0,
        sigdefault: zeros(),
        sigmask: zeros(),
        ignored: zeros(),
        sched_policy: 0,
        sched_prio: 0,
        umask: 0,
        tcpgrp_fd: -1,
        path: b"x".to_vec(),
        cwd: b"/".to_vec(),
        argv: vec![b"x".to_vec(); MAX_ARGS + 1],
        envp: vec![],
        fdmap: vec![],
    });
    assert_eq!(measure(&request), Err(E2BIG));
}

#[test]
fn too_many_argument_bytes_are_rejected() {
    let request = Request::Spawn(SpawnRequest {
        spawn_flags: 0,
        pgroup: 0,
        sigdefault: zeros(),
        sigmask: zeros(),
        ignored: zeros(),
        sched_policy: 0,
        sched_prio: 0,
        umask: 0,
        tcpgrp_fd: -1,
        path: b"x".to_vec(),
        cwd: b"/".to_vec(),
        argv: vec![vec![b'x'; ARG_MAX]],
        envp: vec![],
        fdmap: vec![],
    });
    assert_eq!(measure(&request), Err(E2BIG));
}

#[test]
fn wait_body_decodes() {
    let mut bytes = vec![0u8; 16 + 276];
    bytes[16..20].copy_from_slice(&(0x100i32).to_le_bytes());
    let (_, body) = decode_response(Op::Wait, &bytes).expect("wait");
    assert_eq!(
        body,
        Body::Wait {
            status: 0x100,
            siginfo: [0; 128],
            rusage: [0; 144]
        }
    );
}

#[test]
fn groups_body_decodes() {
    let mut bytes = vec![0u8; 16 + 12];
    bytes[16..20].copy_from_slice(&2u32.to_le_bytes());
    bytes[20..24].copy_from_slice(&7u32.to_le_bytes());
    bytes[24..28].copy_from_slice(&8u32.to_le_bytes());
    let (_, body) = decode_response(Op::GetGroups, &bytes).expect("groups");
    assert_eq!(body, Body::Groups(vec![7, 8]));
}

#[test]
fn rlimit_body_decodes() {
    let mut bytes = vec![0u8; 16 + 16];
    bytes[16..24].copy_from_slice(&1024u64.to_le_bytes());
    bytes[24..32].copy_from_slice(&32768u64.to_le_bytes());
    let (_, body) = decode_response(Op::Rlimit, &bytes).expect("rlimit");
    assert_eq!(
        body,
        Body::Rlimit {
            cur: 1024,
            max: 32768
        }
    );
}

#[test]
fn sched_body_decodes() {
    let mut bytes = vec![0u8; 16 + 20];
    for (index, value) in [9i32, 1, 2, 3, 4].into_iter().enumerate() {
        let offset = 16 + index * 4;
        bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    let (_, body) = decode_response(Op::Sched, &bytes).expect("sched");
    assert_eq!(
        body,
        Body::Sched {
            value: 9,
            param: [1, 2, 3, 4]
        }
    );
}

#[test]
fn hello_body_decodes() {
    let mut bytes = vec![0u8; 16 + 8];
    bytes[16..20].copy_from_slice(&1u32.to_le_bytes());
    bytes[20..24].copy_from_slice(&42i32.to_le_bytes());
    let (_, body) = decode_response(Op::Hello, &bytes).expect("hello");
    assert_eq!(
        body,
        Body::Hello {
            server_version: 1,
            pid: 42
        }
    );
}

#[test]
fn login_body_decodes() {
    let mut bytes = vec![0u8; 16 + 12];
    bytes[16..20].copy_from_slice(&5u32.to_le_bytes());
    bytes[20..25].copy_from_slice(b"alice");
    let (_, body) = decode_response(Op::Login, &bytes).expect("login");
    assert_eq!(body, Body::Login(b"alice".to_vec()));
}

#[test]
fn every_truncated_response_prefix_is_rejected() {
    let bytes = vec![0u8; 16 + 276];
    for length in 0..bytes.len() {
        let result = decode_response(Op::Wait, &bytes[..length]);
        assert!(result.is_err(), "length {length}");
    }
}

#[test]
fn measure_matches_encode_for_random_requests() {
    let mut state = 1u64;
    for _ in 0..1000 {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        let size = usize::try_from(state % 64).unwrap_or(0);
        let title = vec![b'x'; size];
        let request = Request::SetTitle(SetTitleRequest { title });
        let length = measure(&request).expect("measure");
        let mut buffer = vec![0u8; length];
        let encoded = encode(&request, state, &mut buffer).expect("encode");
        assert_eq!(encoded, length);
    }
}
