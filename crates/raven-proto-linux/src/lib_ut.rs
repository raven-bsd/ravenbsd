// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Golden-byte and rejection tests for the Linux trap protocol.

use super::*;

#[test]
fn syscall_golden_bytes_round_trip() {
    let request = LinuxSyscallV1::new(ARCH_X86_64, 1, [1, 2, 3, 4, 5, 6]);
    let mut bytes = [0u8; SYSCALL_LEN];
    assert_eq!(request.encode(&mut bytes), Ok(SYSCALL_LEN));
    assert_eq!(bytes[..2], [1, 1]);
    assert_eq!(bytes[2..10], 1u64.to_le_bytes());
    assert_eq!(bytes[10..18], 1u64.to_le_bytes());
    assert_eq!(bytes[50..58], 6u64.to_le_bytes());
    assert_eq!(LinuxSyscallV1::decode(&bytes), Ok(request));
}

#[test]
fn reply_golden_bytes_round_trip() {
    let reply = LinuxReplyV1::new(-38);
    let mut bytes = [0u8; REPLY_LEN];
    assert_eq!(reply.encode(&mut bytes), Ok(REPLY_LEN));
    assert_eq!(bytes[0], 1);
    assert_eq!(bytes[1..9], (-38i64).to_le_bytes());
    assert_eq!(LinuxReplyV1::decode(&bytes), Ok(reply));
}

#[test]
fn rejects_short_trailing_and_bad_versions() {
    assert_eq!(
        LinuxSyscallV1::decode(&[0u8; SYSCALL_LEN - 1]),
        Err(ProtoError::TooShort)
    );
    assert_eq!(
        LinuxSyscallV1::decode(&[0u8; SYSCALL_LEN + 1]),
        Err(ProtoError::TrailingBytes)
    );
    let mut request = LinuxSyscallV1::new(ARCH_X86_64, 1, [0; 6]);
    request.version = 2;
    let mut bytes = [0u8; SYSCALL_LEN];
    assert_eq!(request.encode(&mut bytes), Err(ProtoError::BadVersion));

    let mut reply = LinuxReplyV1::new(0);
    reply.version = 2;
    let mut bytes = [0u8; REPLY_LEN];
    assert_eq!(reply.encode(&mut bytes), Err(ProtoError::BadVersion));
}

#[test]
fn rejects_unknown_arch_and_too_short_output() {
    let request = LinuxSyscallV1::new(99, 1, [0; 6]);
    let mut bytes = [0u8; SYSCALL_LEN];
    assert_eq!(request.encode(&mut bytes), Err(ProtoError::BadArch));

    let request = LinuxSyscallV1::new(ARCH_AARCH64, 1, [0; 6]);
    let mut short = [0u8; SYSCALL_LEN - 1];
    assert_eq!(request.encode(&mut short), Err(ProtoError::TooShort));
}

#[test]
fn all_arch_ids_encode() {
    for arch in [ARCH_X86_64, ARCH_AARCH64, ARCH_RISCV64] {
        let request = LinuxSyscallV1::new(arch, 0x1234, [7; 6]);
        let mut bytes = [0u8; SYSCALL_LEN];
        assert_eq!(request.encode(&mut bytes), Ok(SYSCALL_LEN));
        assert_eq!(LinuxSyscallV1::decode(&bytes), Ok(request));
    }
}

fn frame() -> LinuxFrame {
    let mut gprs = [0u64; FRAME_GPRS];
    for (index, value) in gprs.iter_mut().enumerate() {
        *value = 0x1000 + index as u64;
    }
    LinuxFrame {
        rip: 0x40_1234,
        rsp: 0x7fff_0000,
        rflags: 0x202,
        fs_base: 0x7000,
        gprs,
    }
}

#[test]
fn frame_golden_bytes_round_trip() {
    let frame = frame();
    let mut bytes = [0u8; 256];
    assert_eq!(frame.encode(&mut bytes), Ok(FRAME_LEN));
    assert_eq!(&bytes[..4], b"LFRM");
    assert_eq!(bytes[4], 1);
    assert_eq!(
        &bytes[FRAME_HEADER..FRAME_HEADER + 8],
        &frame.rip.to_le_bytes()
    );
    assert_eq!(LinuxFrame::decode(&bytes), Ok(frame));
}

#[test]
fn frame_decode_rejects_bad_envelopes() {
    let frame = frame();
    let mut bytes = [0u8; 256];
    frame.encode(&mut bytes).unwrap();
    let mut bad = bytes;
    bad[0] = b'X';
    assert_eq!(LinuxFrame::decode(&bad), Err(ProtoError::BadFrame));
    let mut bad = bytes;
    bad[4] = 2;
    assert_eq!(LinuxFrame::decode(&bad), Err(ProtoError::BadFrame));
    let mut bad = bytes;
    bad[5] = 1;
    assert_eq!(LinuxFrame::decode(&bad), Err(ProtoError::BadFrame));
    assert_eq!(
        LinuxFrame::decode(&bytes[..FRAME_LEN - 1]),
        Err(ProtoError::TooShort)
    );
}
