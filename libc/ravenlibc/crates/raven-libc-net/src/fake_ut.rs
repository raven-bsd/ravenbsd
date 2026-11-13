// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fake netd tests.

use super::*;
use alloc::vec;
use raven_proto_net::Call;

#[test]
fn direct_listen_and_accept() {
    let fake = FakeNetd::new();
    let listener = fake.open("net:tcp/0.0.0.0:8083").expect("listener");
    fake.call(listener, Call::Listen { backlog: 1 })
        .expect("listen");
    let client = fake.open("net:tcp/127.0.0.1:8083").expect("client");
    let _ = client;
    match fake.call(listener, Call::Accept) {
        Ok(CallReply::Accepted(sock)) => {
            assert!(fake.call(sock, Call::GetPeerName).is_ok());
        }
        other => panic!("accept: {other:?}"),
    }
}

#[test]
fn dns_answer_is_built() {
    let fake = FakeNetd::new();
    fake.add_dns_answer(b"example.org", 1, &[192, 0, 2, 1]);
    let sock = fake.open("net:udp/192.0.2.7:53").expect("socket");
    let mut query = vec![0u8; 33];
    query[0] = 0x12;
    query[1] = 0x34;
    query[2] = 0x01;
    query[4] = 0;
    query[5] = 1;
    query[12] = 7;
    query[13..20].copy_from_slice(b"example");
    query[20] = 3;
    query[21..24].copy_from_slice(b"org");
    query[24] = 0;
    query[25] = 0;
    query[26] = 1;
    query[27] = 0;
    query[28] = 1;
    fake.write(sock, &query, None).expect("write");
    let mut buffer = [0u8; 128];
    let read = fake.read(sock, &mut buffer).expect("read");
    let response = buffer.get(..read.len).expect("response");
    assert_eq!(response.get(..2), Some(&[0x12u8, 0x34][..]));
    assert_eq!(response.get(3).copied(), Some(0x80));
    assert_eq!(response.get(7).copied(), Some(1));
    assert_eq!(
        response.get(response.len().saturating_sub(4)..),
        Some(&[192u8, 0, 2, 1][..])
    );
}
