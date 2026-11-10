// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fuzz-style decoder tests: 4,000 deterministic byte strings per decoder.
//!
//! A decoder must never panic. When it accepts an input, re-encoding the decoded value must
//! reproduce the accepted prefix exactly.

use raven_proto_fs::{
    CallRequest, CallResponse, DirEntry, DirEntryHeader, OP_READLINK, OP_STATFS, STAT_LEN, Stat,
};

const ROUNDS: usize = 4_000;

fn fill(seed: &mut u64, out: &mut [u8]) {
    for byte in out {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        *byte = *seed as u8;
    }
}

#[test]
fn decoders_never_panic_and_roundtrip_accepted_input() {
    let mut input = [0u8; 640];
    let mut seed = 0x1234_5678_9abc_def0u64;
    for index in 0..ROUNDS {
        let length = (index * 37 + 11) % input.len();
        fill(&mut seed, &mut input[..length]);
        let bytes = &input[..length];

        if let Ok(stat) = Stat::decode(bytes) {
            let mut encoded = [0u8; STAT_LEN];
            stat.encode(&mut encoded).unwrap();
            assert_eq!(&encoded[..], bytes, "Stat roundtrip at round {index}");
        }

        if let Ok(header) = DirEntryHeader::decode(bytes) {
            let mut encoded = [0u8; 16];
            header.encode(&mut encoded).unwrap();
            assert_eq!(
                &encoded[..],
                bytes,
                "DirEntryHeader roundtrip at round {index}"
            );
        }

        if let Ok((entry, consumed)) = DirEntry::decode(bytes) {
            let mut encoded = [0u8; 272];
            let written = entry.encode(&mut encoded).unwrap();
            assert_eq!(written, consumed, "DirEntry length at round {index}");
            assert_eq!(
                &encoded[..consumed],
                &bytes[..consumed],
                "DirEntry roundtrip at round {index}"
            );
        }

        if let Ok(request) = CallRequest::decode(bytes) {
            let mut encoded = [0u8; 640];
            let written = request.encode(&mut encoded).unwrap();
            assert_eq!(
                &encoded[..written],
                bytes,
                "CallRequest roundtrip at round {index}"
            );
        }

        for op in [OP_READLINK, OP_STATFS] {
            if let Ok(response) = CallResponse::decode(op, bytes) {
                let mut encoded = [0u8; 64];
                let written = response.encode(&mut encoded).unwrap();
                assert_eq!(
                    &encoded[..written],
                    bytes,
                    "CallResponse({op}) roundtrip at round {index}"
                );
            }
        }
    }
}
