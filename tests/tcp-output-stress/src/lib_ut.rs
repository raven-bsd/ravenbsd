// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Data-integrity matrix for the K265 FreeBSD output/persist port.

use super::*;

const MIB: usize = 1024 * 1024;

fn fast_link() -> LinkConfig {
    LinkConfig {
        rtt_ms: 10,
        ..LinkConfig::default()
    }
}

fn assert_stream(received: &[u8], expected: &[u8], label: &str) {
    assert_eq!(received.len(), expected.len(), "{label}: delivered length");
    assert_eq!(
        fnv1a(received),
        fnv1a(expected),
        "{label}: FNV-1a stream hash"
    );
}

#[test]
fn one_mib_both_ways_matches_hash() {
    let (_, mut a, mut b) = pair(StackConfig::default(), fast_link(), 101);
    let (sa, sb) = handshake(&mut a, &mut b);
    let mut now = 200;
    let forward = payload(MIB);
    let received = send_stream(&mut a, &mut b, sa, sb, &forward, &mut now);
    assert_stream(&received, &forward, "1 MiB A->B");

    let backward: Vec<u8> = payload(MIB)
        .into_iter()
        .map(|byte| byte.wrapping_add(0x5a))
        .collect();
    let received = send_stream(&mut b, &mut a, sb, sa, &backward, &mut now);
    assert_stream(&received, &backward, "1 MiB B->A");
}

#[test]
fn sixteen_mib_both_ways_matches_hash() {
    let (_, mut a, mut b) = pair(StackConfig::default(), fast_link(), 102);
    let (sa, sb) = handshake(&mut a, &mut b);
    let mut now = 200;
    let forward = payload(16 * MIB);
    let received = send_stream(&mut a, &mut b, sa, sb, &forward, &mut now);
    assert_stream(&received, &forward, "16 MiB A->B");

    let backward: Vec<u8> = payload(16 * MIB)
        .into_iter()
        .map(|byte| byte.wrapping_add(0x33))
        .collect();
    let received = send_stream(&mut b, &mut a, sb, sa, &backward, &mut now);
    assert_stream(&received, &backward, "16 MiB B->A");
}

#[test]
fn small_writes_both_ways_match_hash() {
    let (_, mut a, mut b) = pair(StackConfig::default(), fast_link(), 103);
    let (sa, sb) = handshake(&mut a, &mut b);
    let records: Vec<Vec<u8>> = (0..2048)
        .map(|index| {
            let length = (index % 8) + 1;
            vec![(index * 7 + 3) as u8; length]
        })
        .collect();
    let expected: Vec<u8> = records.iter().flatten().copied().collect();
    let mut now = 200;
    let received = send_records(&mut a, &mut b, sa, sb, &records, &mut now);
    assert_stream(&received, &expected, "small writes A->B");

    let reverse: Vec<Vec<u8>> = records.iter().rev().cloned().collect();
    let expected: Vec<u8> = reverse.iter().flatten().copied().collect();
    let received = send_records(&mut b, &mut a, sb, sa, &reverse, &mut now);
    assert_stream(&received, &expected, "small writes B->A");
}

#[test]
fn zero_window_probe_reopen_matches_hash() {
    let config = StackConfig {
        rx_queue_bytes: 2048,
        ..StackConfig::default()
    };
    let (_, mut a, mut b) = pair(config, fast_link(), 104);
    let (sa, sb) = handshake(&mut a, &mut b);
    let mut now = 200;
    let data = payload(8192);
    assert_eq!(a.send(sa, &data).expect("send"), data.len());

    // Let the receiver window close and the persist timer fire at the 1 s minimum.
    pump(&mut a, &mut b, now, now + 1400);
    now += 1400;
    let probes = a.test_tcp_zero_window_probes(sa).unwrap_or(0);
    assert!(probes >= 1, "the persist probe did not fire: {probes}");

    let mut received = Vec::new();
    let mut buffer = vec![0u8; 512];
    let deadline = now + 120_000;
    while received.len() < data.len() {
        now += 10;
        a.poll(Instant::from_millis(now));
        b.poll(Instant::from_millis(now));
        drain(&mut b, sb, &mut received, &mut buffer);
        assert!(now < deadline, "zero-window reopen stalled");
    }
    assert_stream(&received, &data, "zero-window reopen");
}

#[test]
fn retransmit_loss_matches_hash() {
    let link = LinkConfig {
        rtt_ms: 10,
        loss_percent: 5.0,
        ..LinkConfig::default()
    };
    let (_, mut a, mut b) = pair(StackConfig::default(), link, 105);
    let (sa, sb) = handshake(&mut a, &mut b);
    let mut now = 200;
    let data = payload(MIB);
    let received = send_stream(&mut a, &mut b, sa, sb, &data, &mut now);
    assert_stream(&received, &data, "5% loss A->B");
    assert!(
        a.counters().retrans_segments > 0,
        "the loss scenario produced no retransmission"
    );
}

#[test]
fn sack_loss_matches_hash() {
    let link = LinkConfig {
        rtt_ms: 10,
        loss_percent: 2.0,
        ..LinkConfig::default()
    };
    let (_, mut a, mut b) = pair(StackConfig::default(), link, 106);
    let (sa, sb) = handshake(&mut a, &mut b);
    let mut now = 200;
    let data = payload(MIB);
    assert_eq!(a.test_tcp_sack_permitted(sa), Some(true));
    let received = send_stream(&mut a, &mut b, sa, sb, &data, &mut now);
    assert_stream(&received, &data, "2% SACK loss A->B");
    assert!(
        a.counters().retrans_segments > 0,
        "the SACK loss scenario produced no retransmission"
    );
}

#[test]
fn persist_cycles_match_hash() {
    let data = payload(2 * 44);
    let mut received = Vec::new();
    for cycle in 0..2 {
        // A fresh connection per cycle isolates the persist state.
        let (_, mut a, mut b) = pair(StackConfig::default(), fast_link(), 107 + cycle as u64);
        let (sa, sb) = handshake(&mut a, &mut b);
        let mut now = 200;
        // Model a stale/closed peer window; the real peer window is open, so the persist
        // probe is accepted and its ACK reopens the sender in the same cycle.
        assert!(b.test_set_tcp_snd_wnd(sb, 0));
        let before = b.test_tcp_zero_window_probes(sb).unwrap_or(0);
        assert_eq!(
            b.send(sb, &data[cycle * 44..(cycle + 1) * 44])
                .expect("send"),
            44
        );
        pump(&mut a, &mut b, now, now + 1200);
        now += 1200;
        pump(&mut a, &mut b, now, now + 300);
        now += 300;
        let after = b.test_tcp_zero_window_probes(sb).unwrap_or(0);
        assert!(
            after > before,
            "persist cycle {cycle} did not probe: {before} -> {after}"
        );
        let mut buffer = vec![0u8; 64];
        let mut stream = Vec::new();
        let deadline = now + 5_000;
        while stream.len() < 44 {
            now += 5;
            a.poll(Instant::from_millis(now));
            b.poll(Instant::from_millis(now));
            drain(&mut a, sa, &mut stream, &mut buffer);
            assert!(now < deadline, "persist cycle {cycle} did not reopen");
        }
        assert_eq!(stream.len(), 44, "persist cycle {cycle} stream length");
        received.extend_from_slice(&stream);
    }
    assert_stream(&received, &data, "persist cycles");
}

#[test]
fn a_one_byte_send_offset_corruption_is_detected() {
    let (_, mut a, mut b) = pair(StackConfig::default(), fast_link(), 108);
    let (sa, sb) = handshake(&mut a, &mut b);
    let mut now = 200;
    let data = payload(4096);
    let received = send_stream(&mut a, &mut b, sa, sb, &data, &mut now);
    assert_stream(&received, &data, "uncorrupted baseline");

    let mut corrupted = data.clone();
    corrupted[1234] ^= 0xff;
    let (_, mut c, mut d) = pair(StackConfig::default(), fast_link(), 109);
    let (sc, sd) = handshake(&mut c, &mut d);
    let mut now = 200;
    let received = send_stream(&mut c, &mut d, sc, sd, &corrupted, &mut now);
    assert_stream(&received, &corrupted, "corrupted send is delivered as sent");
    assert_ne!(
        fnv1a(&received),
        fnv1a(&data),
        "the stream hash must catch a one-byte send-offset corruption"
    );
}
