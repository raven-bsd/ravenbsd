// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Scheme client tests.

use raven_abi::errno::Errno;
use raven_abi::types::Handle;

use crate::backend::{FakeSys, SysBackend};
use crate::fd::ofd::OfdKind;
use crate::scheme::client::{decode_fs_response, encode_fs_call, fstat_record, open_scheme_path};
use crate::scheme::probe::probe_kind;
use crate::xlat;

#[test]
fn encode_a_mkdir_call() {
    let mut out = [0u8; 128];
    let length = encode_fs_call(
        raven_proto_fs::Call::Mkdir {
            mode: 0o755,
            path: b"/tmp/x",
        },
        &mut out,
    )
    .expect("encode");
    assert!(length >= raven_proto_fs::CALL_HEADER_LEN);
    let decoded = raven_proto_fs::CallRequest::decode(&out[..length]).expect("decode");
    match decoded.call {
        raven_proto_fs::Call::Mkdir { mode, path } => {
            assert_eq!(mode, 0o755);
            assert_eq!(path, b"/tmp/x");
        }
        _ => panic!("wrong op"),
    }
}

#[test]
fn decode_a_status_response() {
    let mut bytes = [0u8; 8];
    let length = raven_proto_fs::CallResponse::Status(-2)
        .encode(&mut bytes)
        .expect("encode");
    assert_eq!(
        decode_fs_response(raven_proto_fs::OP_SYNC, &bytes[..length]),
        Err(Errno::ENOENT)
    );
}

#[test]
fn undecodable_response_is_eproto() {
    assert_eq!(
        decode_fs_response(raven_proto_fs::OP_SYNC, &[0, 0, 0, 0, 1]),
        Err(Errno::EPROTO)
    );
}

#[test]
fn open_scheme_path_does_not_add_file() {
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let handle =
        open_scheme_path(&fake, b"file:/dev/tty", raven_proto_fs::WIRE_OPEN_READ).expect("open");
    assert_eq!(handle, Handle(5));
}

#[test]
fn fstat_record_decodes() {
    let record = [0u8; 64];
    assert!(fstat_record(&record).is_ok());
}

#[test]
fn probe_detects_a_tty() {
    let fake = FakeSys::new();
    fake.tty(b"/dev/tty");
    let handle = fake
        .open(b"file:/dev/tty", u32::from(raven_proto_fs::WIRE_OPEN_READ))
        .expect("open");
    assert_eq!(probe_kind(&fake, handle), OfdKind::Tty);
}

#[test]
fn probe_leaves_files_unknown() {
    let fake = FakeSys::new();
    fake.file(b"/a", b"x");
    let handle = fake
        .open(b"file:/a", u32::from(raven_proto_fs::WIRE_OPEN_READ))
        .expect("open");
    assert_eq!(probe_kind(&fake, handle), OfdKind::Unknown);
}

#[test]
fn call_status_helper_is_used() {
    assert_eq!(xlat::call_status(0), Ok(()));
}
