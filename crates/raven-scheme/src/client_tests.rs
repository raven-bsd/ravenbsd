// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the client wrappers.

use super::*;
use crate::testing::FakeKernel;
use alloc::vec;
use raven_abi::types::Handle;
use raven_proto_fs::{Call, CallRequest, Stat};

#[test]
fn open_records_path_and_flags() {
    let kernel = FakeKernel::new();
    let client = Client::with_kernel(kernel.clone());
    let handle = client.open("file:/tmp/a", 3).unwrap();
    assert_eq!(handle, Handle(1));
    assert_eq!(kernel.open_calls(), vec![("file:/tmp/a".to_string(), 3)]);
}

#[test]
fn read_returns_scripted_bytes() {
    let kernel = FakeKernel::new();
    kernel.set_read_data(b"hello");
    let client = Client::with_kernel(kernel);
    let mut buffer = [0u8; 3];
    assert_eq!(client.read(Handle(1), 0, &mut buffer).unwrap(), 3);
    assert_eq!(&buffer, b"hel");
}

#[test]
fn write_records_bytes() {
    let kernel = FakeKernel::new();
    let client = Client::with_kernel(kernel.clone());
    assert_eq!(client.write(Handle(1), 0, b"abc").unwrap(), 3);
    assert_eq!(kernel.written(), b"abc");
}

#[test]
fn close_records_the_handle() {
    let kernel = FakeKernel::new();
    let client = Client::with_kernel(kernel.clone());
    client.close(Handle(7)).unwrap();
    assert_eq!(kernel.closed(), vec![Handle(7)]);
}

#[test]
fn dup_returns_a_new_handle() {
    let kernel = FakeKernel::new();
    let client = Client::with_kernel(kernel);
    assert_eq!(client.dup(Handle(1), 0xff).unwrap(), Handle(1));
}

#[test]
fn fstat_decodes_the_record() {
    let kernel = FakeKernel::new();
    let stat = Stat {
        ino: 9,
        size: 10,
        blocks: 1,
        mtime: 1,
        ctime: 2,
        atime: 3,
        mode: 0o100644,
        nlink: 1,
        uid: 0,
        gid: 0,
    };
    let mut bytes = [0u8; 64];
    stat.encode(&mut bytes).unwrap();
    kernel.set_fstat(bytes);
    let client = Client::with_kernel(kernel);
    assert_eq!(client.fstat(Handle(1)).unwrap(), stat);
}

#[test]
fn call_returns_scripted_bytes() {
    let kernel = FakeKernel::new();
    kernel.set_call_response(b"reply");
    let client = Client::with_kernel(kernel);
    let mut response = [0u8; 8];
    assert_eq!(
        client.call(Handle(1), b"request", &mut response).unwrap(),
        5
    );
    assert_eq!(&response[..5], b"reply");
}

#[test]
fn call_typed_encodes_a_k60_request() {
    let kernel = FakeKernel::new();
    kernel.set_call_response(b"ok");
    let client = Client::with_kernel(kernel);
    let request = CallRequest::new(Call::Sync);
    let mut response = [0u8; 2];
    assert_eq!(
        client
            .call_typed(Handle(1), &request, &mut response)
            .unwrap(),
        2
    );
    assert_eq!(&response, b"ok");
}
