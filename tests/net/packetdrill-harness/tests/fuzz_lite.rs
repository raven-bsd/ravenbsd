// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Fuzz-lite: random and truncated frames are fed through the harness link; the stack must not
//! panic. The generator is deterministic so a failing seed can be replayed.

use raven_netcore_packetdrill_harness::frame::{
    ACK, PEER_IP, PEER_MAC, PEER_PORT, PSH, SUT_IP, SUT_MAC, SUT_PORT, SYN, build_tcp,
};
use raven_netcore_packetdrill_harness::{Runner, parse};

fn runner() -> Runner {
    let script = parse(
        "fuzz.pkt",
        "0.000 socket(..., SOCK_STREAM, IPPROTO_TCP) = 3\n\
         +0.000 bind(3, ..., ...) = 0\n\
         +0.000 listen(3, 1) = 0\n",
    )
    .expect("fuzz preamble");
    Runner::new(script)
}

fn next(seed: &mut u64) -> u64 {
    let mut value = *seed;
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    *seed = value;
    value
}

#[test]
fn random_ethernet_frames_never_panic() {
    let mut runner = runner();
    let mut seed = 0x0bad_c0de_dead_beefu64;
    for index in 0..2000 {
        let length = 14 + (next(&mut seed) as usize % 120);
        let mut frame = vec![0u8; length];
        frame[0..6].copy_from_slice(&SUT_MAC);
        frame[6..12].copy_from_slice(&PEER_MAC);
        frame[12] = (next(&mut seed) & 0xff) as u8;
        frame[13] = (next(&mut seed) & 0xff) as u8;
        for byte in &mut frame[14..] {
            *byte = (next(&mut seed) & 0xff) as u8;
        }
        runner
            .inject_raw(&frame)
            .unwrap_or_else(|error| panic!("frame {index} panicked/failed: {error:#}"));
    }
}

#[test]
fn truncated_tcp_frames_never_panic() {
    let mut runner = runner();
    let frame = build_tcp(
        PEER_MAC,
        SUT_MAC,
        PEER_IP,
        SUT_IP,
        PEER_PORT,
        SUT_PORT,
        1000,
        1,
        SYN,
        65535,
        &[],
    );
    for length in 0..frame.len() {
        runner
            .inject_raw(&frame[..length])
            .unwrap_or_else(|error| panic!("truncated frame of {length} bytes failed: {error:#}"));
    }
    runner
        .inject_raw(&frame)
        .expect("the complete SYN must still be accepted");
}

#[test]
fn garbage_before_a_valid_handshake_is_harmless() {
    let mut runner = runner();
    for kind in 0..64u8 {
        let mut frame = vec![0u8; 60];
        frame[0..6].copy_from_slice(&SUT_MAC);
        frame[6..12].copy_from_slice(&PEER_MAC);
        frame[12..14].copy_from_slice(&[0x08, 0x00]);
        frame[14] = 0x45;
        frame[14 + 9] = kind;
        runner.inject_raw(&frame).expect("garbage frame");
    }

    let syn = build_tcp(
        PEER_MAC,
        SUT_MAC,
        PEER_IP,
        SUT_IP,
        PEER_PORT,
        SUT_PORT,
        1000,
        0,
        SYN,
        65535,
        &[],
    );
    runner.inject_raw(&syn).expect("SYN after garbage");
    let ack = build_tcp(
        PEER_MAC,
        SUT_MAC,
        PEER_IP,
        SUT_IP,
        PEER_PORT,
        SUT_PORT,
        1001,
        0,
        ACK,
        65535,
        &[],
    );
    runner.inject_raw(&ack).expect("ACK after garbage");
    let data = build_tcp(
        PEER_MAC,
        SUT_MAC,
        PEER_IP,
        SUT_IP,
        PEER_PORT,
        SUT_PORT,
        1001,
        0,
        ACK | PSH,
        65535,
        b"hello",
    );
    runner.inject_raw(&data).expect("data after garbage");
}
