// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the version-1 `proc:` codec.

#![allow(clippy::indexing_slicing)] // fixed test buffers
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
        path: b"/bin/procd-child".to_vec(),
        cwd: b"/".to_vec(),
        argv: vec![b"procd-child".to_vec(), b"exec".to_vec()],
        envp: vec![b"TEST=value".to_vec()],
        fdmap: vec![FdMapEntry { fd: 0, handle: 1 }],
    })
}

fn header(version: u16, op: u16, flags: u32, seq: u64) -> [u8; 16] {
    let mut bytes = [0u8; 16];
    bytes[0..2].copy_from_slice(&version.to_le_bytes());
    bytes[2..4].copy_from_slice(&op.to_le_bytes());
    bytes[4..8].copy_from_slice(&flags.to_le_bytes());
    bytes[8..16].copy_from_slice(&seq.to_le_bytes());
    bytes
}

#[test]
fn hello_wait_kill_and_group_requests_round_trip() {
    let cases = [
        Request::Hello(HelloRequest { client_version: 1 }),
        Request::Wait(WaitRequest {
            idtype: 1,
            id: 42,
            options: 0,
        }),
        Request::Kill(KillRequest {
            pid: 42,
            sig: 15,
            flags: 0,
            value: None,
        }),
        Request::Kill(KillRequest {
            pid: 42,
            sig: 15,
            flags: 1,
            value: Some(0x1234),
        }),
        Request::SetPgid(SetPgidRequest { pid: 42, pgid: 42 }),
        Request::GetPgid(GetPgidRequest { pid: 42 }),
        Request::GetSid(GetSidRequest { pid: 42 }),
    ];
    for (index, request) in cases.iter().enumerate() {
        let mut bytes = [0u8; 1024];
        let length = encode_request(request, index as u64 + 1, &mut bytes).expect("encode");
        let (decoded_header, decoded) = decode_request(&bytes[..length]).expect("decode");
        assert_eq!(decoded_header.seq, index as u64 + 1);
        assert_eq!(decoded_header.op, request.op());
        assert_eq!(&decoded, request);
    }
}

#[test]
fn spawn_round_trip_preserves_arguments_environment_and_fdmap() {
    let request = spawn();
    let mut bytes = [0u8; 1024];
    let length = encode_request(&request, 7, &mut bytes).expect("encode");
    let (header, decoded) = decode_request(&bytes[..length]).expect("decode");
    assert_eq!(header.seq, 7);
    assert_eq!(header.op, Op::Spawn);
    assert_eq!(decoded, request);
}

#[test]
fn known_but_unimplemented_operations_are_identified() {
    for op in [
        Op::Fork,
        Op::Exec,
        Op::SetSid,
        Op::SetCred,
        Op::GetGroups,
        Op::Rlimit,
        Op::Rusage,
        Op::Prio,
        Op::Sched,
        Op::SetTitle,
        Op::Prctl,
        Op::Login,
        Op::Pledge,
        Op::CapEnter,
        Op::SessionInfo,
    ] {
        let bytes = header(VERSION, op as u16, 0, 9);
        let (decoded_header, decoded) = decode_request(&bytes).expect("decode");
        assert_eq!(decoded_header.op, op);
        assert_eq!(decoded, Request::Other(op));
    }
}

#[test]
fn malformed_requests_are_rejected() {
    let mut bytes = [0u8; 1024];
    let length = encode_request(&spawn(), 1, &mut bytes).expect("encode");

    assert_eq!(decode_request(&bytes[..length - 1]), Err(Errno::EINVAL));

    let bad_version = header(2, Op::Hello as u16, 0, 1);
    assert_eq!(decode_request(&bad_version), Err(Errno::EPROTO));

    let bad_op = header(VERSION, 0x77, 0, 1);
    assert_eq!(decode_request(&bad_op), Err(Errno::EINVAL));

    let short = [0u8; 8];
    assert_eq!(decode_request(&short), Err(Errno::EINVAL));
}

#[test]
fn response_bodies_round_trip() {
    let wait = Response::body(
        42,
        Body::Wait {
            status: 7 << 8,
            siginfo: [1u8; 128],
            rusage: [2u8; 144],
        },
    );
    let mut bytes = [0u8; 512];
    let length = encode_response(&wait, &mut bytes).expect("encode");
    assert_eq!(decode_response(Op::Wait, &bytes[..length]), Ok(wait));

    let hello = Response::body(
        0,
        Body::Hello {
            server_version: 1,
            pid: 3,
        },
    );
    let length = encode_response(&hello, &mut bytes).expect("encode");
    assert_eq!(decode_response(Op::Hello, &bytes[..length]), Ok(hello));

    let groups = Response::body(3, Body::Groups(vec![0, 100, 1000]));
    let length = encode_response(&groups, &mut bytes).expect("encode");
    assert_eq!(decode_response(Op::GetGroups, &bytes[..length]), Ok(groups));
}

#[test]
fn error_responses_and_small_buffers_are_handled() {
    let error = Response::error(Errno::ENOSYS);
    let mut bytes = [0u8; 64];
    let length = encode_response(&error, &mut bytes).expect("encode");
    assert_eq!(length, 16);
    assert_eq!(
        decode_response(Op::Fork, &bytes[..length]),
        Ok(Response {
            status: -(Errno::ENOSYS.0 as i32),
            value: 0,
            body: Body::Empty,
        })
    );

    let mut tiny = [0u8; 8];
    assert_eq!(
        encode_response(&Response::ok(0), &mut tiny),
        Err(Errno::E2BIG)
    );
}

#[test]
fn oversized_strings_are_rejected() {
    let mut request = SpawnRequest {
        spawn_flags: 0,
        pgroup: 0,
        sigdefault: zeros(),
        sigmask: zeros(),
        ignored: zeros(),
        sched_policy: 0,
        sched_prio: 0,
        umask: 0,
        tcpgrp_fd: -1,
        path: vec![b'x'; MAX_STRING + 1],
        cwd: b"/".to_vec(),
        argv: vec![],
        envp: vec![],
        fdmap: vec![],
    };
    let mut bytes = [0u8; 8192];
    assert_eq!(
        encode_request(&Request::Spawn(request.clone()), 1, &mut bytes),
        Err(Errno::ENAMETOOLONG)
    );
    request.path = b"/bin/ok".to_vec();
    assert!(encode_request(&Request::Spawn(request), 1, &mut bytes).is_ok());
}
