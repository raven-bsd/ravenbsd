// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Random-input robustness tests.
//!
//! Every decoder is fed 4,000 deterministic pseudo-random buffers. A decoder may accept or reject
//! any of them, but it must never panic and must never read outside the input.

use raven_proto_net::{
    Desc, IfaceRequest, IfaceResponse, Response, RingHeader, SchemePath, SocketRequest, path,
};

/// Small deterministic xorshift generator; a failing seed can be replayed.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, limit: usize) -> usize {
        if limit == 0 {
            return 0;
        }
        (self.next() % limit as u64) as usize
    }

    fn bytes(&mut self, limit: usize) -> Vec<u8> {
        let length = self.below(limit.saturating_add(1));
        let mut buffer = vec![0u8; length];
        for byte in &mut buffer {
            *byte = (self.next() & 0xff) as u8;
        }
        buffer
    }
}

#[test]
fn socket_requests_never_panic() {
    let mut rng = Rng(0x5eed_0001_abcd_9876);
    for _ in 0..4000 {
        let buffer = rng.bytes(96);
        let _ = SocketRequest::decode(&buffer);
    }
}

#[test]
fn responses_never_panic() {
    let mut rng = Rng(0x5eed_0002_abcd_9876);
    for _ in 0..4000 {
        let buffer = rng.bytes(96);
        let _ = Response::decode(&buffer);
    }
}

#[test]
fn interface_requests_never_panic() {
    let mut rng = Rng(0x5eed_0003_abcd_9876);
    for _ in 0..4000 {
        let buffer = rng.bytes(96);
        let _ = IfaceRequest::decode(&buffer);
    }
}

#[test]
fn interface_responses_never_panic() {
    let mut rng = Rng(0x5eed_0004_abcd_9876);
    for _ in 0..4000 {
        let buffer = rng.bytes(96);
        let _ = IfaceResponse::decode(&buffer);
    }
}

#[test]
fn ring_header_and_descriptor_never_panic() {
    let mut rng = Rng(0x5eed_0005_abcd_9876);
    for _ in 0..4000 {
        let buffer = rng.bytes(160);
        let _ = RingHeader::read_from(&buffer);
        let _ = Desc::read_from(&buffer);
    }
}

#[test]
fn random_paths_never_panic() {
    let mut rng = Rng(0x5eed_0006_abcd_9876);
    for _ in 0..4000 {
        let buffer = rng.bytes(80);
        let text = String::from_utf8_lossy(&buffer);
        let _: Option<SchemePath> = path::parse(&text).ok();
    }
}

#[test]
fn valid_prefixes_of_golden_messages_do_not_panic() {
    let mut buffer = [0u8; 128];
    let length = SocketRequest::Call(raven_proto_net::Call::Accept)
        .encode(&mut buffer)
        .unwrap();
    for prefix in 0..=length {
        let _ = SocketRequest::decode(&buffer[..prefix]);
    }
    let length = Response::Ok.encode(&mut buffer).unwrap();
    for prefix in 0..=length {
        let _ = Response::decode(&buffer[..prefix]);
    }
}
