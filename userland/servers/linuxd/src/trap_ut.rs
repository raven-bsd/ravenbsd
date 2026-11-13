// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Trap codec tests: wire bytes, dispatch decisions and the copy-in size.

use raven_proto_linux::{LinuxSyscallV1, SYSCALL_LEN};

use super::*;
use crate::handlers::dispatcher;

fn request(nr: u64, args: [u64; 6]) -> [u8; SYSCALL_LEN] {
    let mut bytes = [0u8; SYSCALL_LEN];
    LinuxSyscallV1::new(raven_proto_linux::ARCH_X86_64, nr, args)
        .encode(&mut bytes)
        .expect("a full request buffer");
    bytes
}

fn process() -> LinuxProcess {
    LinuxProcess::new(
        7,
        raven_abi::types::Handle(9),
        0x4000,
        0x5000,
        0x8000,
        0x9000,
    )
}

#[test]
fn a_write_request_decodes_and_replies() {
    let mut process = process();
    process.user_buffer.extend_from_slice(b"hello\n");
    let mut dispatcher = dispatcher();
    let bytes = request(1, [1, 0x1000, 6, 0, 0, 0]);
    let call = decode(&bytes).expect("a valid request");
    assert_eq!(call.nr, 1);
    assert_eq!(copy_in_len(&call), Some(6));
    let action = dispatch(&mut process, &mut dispatcher, &call);
    assert_eq!(action, TrapAction::Reply(6));
    assert_eq!(process.output, b"hello\n");
    let mut reply = [0u8; 16];
    assert_eq!(encode_reply(6, &mut reply), 9);
    assert_eq!(LinuxReplyV1::decode(&reply[..9]).expect("reply").value, 6);
}

#[test]
fn exit_group_becomes_an_exit_action() {
    let mut process = process();
    let mut dispatcher = dispatcher();
    let bytes = request(231, [0, 0, 0, 0, 0, 0]);
    let call = decode(&bytes).expect("a valid request");
    assert_eq!(
        dispatch(&mut process, &mut dispatcher, &call),
        TrapAction::Exit(0)
    );
}

#[test]
fn copy_in_is_bounded_by_the_server_limit() {
    let call = LinuxSyscallV1::new(
        raven_proto_linux::ARCH_X86_64,
        1,
        [1, 0x1000, u64::MAX, 0, 0, 0],
    );
    assert_eq!(copy_in_len(&call), Some(crate::table::USER_BUFFER_MAX));
    let other = LinuxSyscallV1::new(raven_proto_linux::ARCH_X86_64, 39, [0; 6]);
    assert_eq!(copy_in_len(&other), None);
}

#[test]
fn a_bad_request_is_rejected() {
    assert_eq!(
        decode(&[0u8; 10]),
        Err(TrapError::Proto(raven_proto_linux::ProtoError::TooShort))
    );
    let mut bytes = request(1, [1, 0, 1, 0, 0, 0]);
    bytes[0] = 9;
    assert_eq!(
        decode(&bytes),
        Err(TrapError::Proto(raven_proto_linux::ProtoError::BadVersion))
    );
    let mut bytes = request(1, [1, 0, 1, 0, 0, 0]);
    bytes[1] = 9;
    assert_eq!(
        decode(&bytes),
        Err(TrapError::Proto(raven_proto_linux::ProtoError::BadArch))
    );
}

#[test]
fn the_lengths_match_the_wire_format() {
    let (request_len, reply_len) = lengths();
    assert_eq!(request_len, 58);
    assert_eq!(reply_len, 9);
}
