// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the pfctl ABI codec.

use super::*;

#[test]
fn requests_round_trip() {
    let mut buffer = [0u8; 256];
    let cases = [
        Request::Load(b"pass in all\n".to_vec()),
        Request::Stats,
        Request::ClearStats,
        Request::StateCount,
        Request::RuleStats(7),
    ];
    for request in cases {
        let length = encode_request(&request, &mut buffer).unwrap();
        assert_eq!(decode_request(&buffer[..length]), Some(request));
    }
    assert_eq!(decode_request(&[0]), None);
    assert_eq!(decode_request(&[0xff, 0xff]), None);
}

#[test]
fn responses_round_trip() {
    let mut buffer = [0u8; 256];
    let stats = CounterSnapshot {
        in_pass: 1,
        in_block: 2,
        out_pass: 3,
        out_block: 4,
        states_created: 5,
        states_expired: 6,
        state_hits: 7,
    };
    let cases = [
        Response::Ok,
        Response::Stats(stats),
        Response::StateCount(9),
        Response::RuleStats(1, 2, 3, 4),
        Response::Error(42),
    ];
    for response in cases {
        let length = encode_response(&response, &mut buffer).unwrap();
        assert_eq!(decode_response(&buffer[..length]), Some(response));
    }
    assert_eq!(decode_response(&[0xff, 0xff]), None);
    assert!(encode_request(&Request::Load(vec![0; MAX_PAYLOAD + 1]), &mut buffer).is_none());
}
