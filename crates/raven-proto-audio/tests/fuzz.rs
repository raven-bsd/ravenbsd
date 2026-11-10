// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Random-input robustness tests.
//!
//! Every decoder is fed 4,000 deterministic pseudo-random buffers. A decoder may accept or reject
//! any of them, but it must never panic and must never read outside the input.

use raven_proto_audio::{
    BufferInfo, RingHeader, Status, StreamRequest, StreamResponse, StreamState, format, path,
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
fn stream_requests_never_panic() {
    let mut rng = Rng(0xa0d1_0001_abcd_9876);
    for _ in 0..4000 {
        let buffer = rng.bytes(192);
        let _ = StreamRequest::decode(&buffer);
    }
}

#[test]
fn stream_responses_never_panic() {
    let mut rng = Rng(0xa0d1_0002_abcd_9876);
    for _ in 0..4000 {
        let buffer = rng.bytes(192);
        let _ = StreamResponse::decode(&buffer);
    }
}

#[test]
fn ring_headers_never_panic() {
    let mut rng = Rng(0xa0d1_0003_abcd_9876);
    for _ in 0..4000 {
        let buffer = rng.bytes(256);
        let _ = RingHeader::read_from(&buffer);
    }
}

#[test]
fn random_paths_never_panic() {
    let mut rng = Rng(0xa0d1_0004_abcd_9876);
    for _ in 0..4000 {
        let buffer = rng.bytes(64);
        let text = String::from_utf8_lossy(&buffer);
        let _ = path::parse(&text);
    }
}

#[test]
fn random_formats_never_panic() {
    let mut rng = Rng(0xa0d1_0005_abcd_9876);
    for _ in 0..4000 {
        let _ = format::SampleFormat::from_code((rng.next() & 0xff) as u8);
        let _ = format::Format {
            rate: (rng.next() & 0xffff_ffff) as u32,
            channels: (rng.next() & 0xff) as u8,
            sample: format::SampleFormat::S16Le,
        }
        .check();
    }
}

#[test]
fn valid_prefixes_of_golden_messages_do_not_panic() {
    let mut buffer = [0u8; 192];
    let length = StreamRequest::Write { data: b"pcm" }
        .encode(&mut buffer)
        .unwrap();
    for prefix in 0..=length {
        let _ = StreamRequest::decode(&buffer[..prefix]);
    }
    let length = StreamResponse::Status(Status {
        position_frames: 7,
        underruns: 1,
        state: StreamState::Running,
    })
    .encode(&mut buffer)
    .unwrap();
    for prefix in 0..=length {
        let _ = StreamResponse::decode(&buffer[..prefix]);
    }
    let length = StreamResponse::Buffer(BufferInfo {
        period_frames: 256,
        period_count: 2,
        region_len: 2_176,
    })
    .encode(&mut buffer)
    .unwrap();
    for prefix in 0..=length {
        let _ = StreamResponse::decode(&buffer[..prefix]);
    }
}
