// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Loss/retransmit/reordering matrix and dedicated scenarios.

use super::*;

fn fast_link(rtt: u64) -> LinkConfig {
    LinkConfig {
        rtt_ms: rtt,
        ..LinkConfig::default()
    }
}

#[test]
fn ten_thousand_random_loss_reorder_sequences_hash_the_stream() {
    let mut loss_cases = 0usize;
    let mut reorder_cases = 0usize;
    let mut duplicate_cases = 0usize;
    let mut stack_retransmit_cases = 0usize;
    let mut trace_retransmit_cases = 0usize;
    let mut rto_cases = 0usize;
    let mut fast_cases = 0usize;
    for seed in 0..10_000u64 {
        let scenario = Scenario::new(seed);
        let link_config = LinkConfig {
            rtt_ms: scenario.rtt,
            duplicate_percent: scenario.duplicate,
            reorder_percent: scenario.reorder,
            ..LinkConfig::default()
        };
        let (link, mut a, mut b, lossy) =
            pair_lossy(StackConfig::default(), link_config, seed, scenario.loss);
        let (sa, sb) = handshake(&mut a, &mut b);
        let (_, first_seq, _) = a.test_tcp_window(sa).expect("window");
        lossy.set(true);
        let mut now = 60;
        let received = send_stream(&mut a, &mut b, sa, sb, &scenario.payload, &mut now, seed);
        assert_eq!(
            received.len(),
            scenario.payload.len(),
            "seed {seed}: delivered length"
        );
        assert_eq!(
            fnv1a(&received),
            fnv1a(&scenario.payload),
            "seed {seed}: stream hash"
        );
        let stats = validate_trace(&link.trace(), &scenario.payload, first_seq)
            .unwrap_or_else(|error| panic!("seed {seed}: {error}"));
        if scenario.loss > 0.0 {
            loss_cases += 1;
        }
        if scenario.reorder > 0.0 {
            reorder_cases += 1;
        }
        if scenario.duplicate > 0.0 {
            duplicate_cases += 1;
        }
        if a.counters().retrans_segments > 0 {
            stack_retransmit_cases += 1;
        }
        if stats.retransmits > 0 {
            trace_retransmit_cases += 1;
        }
        if a.counters().rto_fires > 0 {
            rto_cases += 1;
        }
        if a.counters().fast_retransmits > 0 {
            fast_cases += 1;
        }
    }
    eprintln!(
        "K317 seeds=10000 loss={loss_cases} reorder={reorder_cases} duplicate={duplicate_cases} \
         stack_retransmits={stack_retransmit_cases} trace_overlaps={trace_retransmit_cases} \
         rto={rto_cases} fast={fast_cases}"
    );
    assert!(loss_cases > 5_000);
    assert!(reorder_cases > 5_000);
    assert!(duplicate_cases > 1_000);
    assert!(stack_retransmit_cases > 500);
}

#[test]
fn duplicate_acks_trigger_a_fast_retransmit() {
    let (link, mut a, mut b) = pair(StackConfig::default(), fast_link(10), 2_001);
    let (sa, sb) = handshake(&mut a, &mut b);
    let data = payload(2_001, 4_096);
    assert_eq!(a.send(sa, &data).expect("send"), data.len());
    let (_, snd_una, snd_nxt) = a.test_tcp_window(sa).expect("window");
    assert!(snd_nxt > snd_una, "the data is in flight");
    let b_seq = b.test_tcp_window(sb).expect("b window").2;
    for step in 0..3 {
        inject_duplicate_ack(&mut a, b_seq, snd_una, 61 + step);
    }
    a.poll(Instant::from_millis(64));
    assert!(
        a.counters().fast_retransmits >= 1,
        "three duplicate ACKs did not trigger a fast retransmit"
    );
    let mut now = 64;
    let mut received = Vec::new();
    let mut buffer = vec![0u8; 65_536];
    while received.len() < data.len() {
        now += 1;
        a.poll(Instant::from_millis(now));
        b.poll(Instant::from_millis(now));
        drain(&mut b, sb, &mut received, &mut buffer);
        assert!(now < 5_000, "fast-retransmit stream stalled");
    }
    assert_eq!(fnv1a(&received), fnv1a(&data));
    let stats = validate_trace(&link.trace(), &data, snd_una).expect("oracle");
    assert!(
        stats.retransmits >= 1,
        "the fast retransmit is not on the wire"
    );
}

#[test]
fn rto_recovers_a_stream_dropped_by_the_device() {
    let (link, mut a, mut b, drop) =
        pair_lossy(StackConfig::default(), fast_link(10), 2_002, 100.0);
    let (sa, sb) = handshake(&mut a, &mut b);
    let (_, first_seq, _) = a.test_tcp_window(sa).expect("window");
    let data = payload(2_002, 1_024);
    drop.set(true);
    assert_eq!(a.send(sa, &data).expect("send"), data.len());
    pump(&mut a, &mut b, 60, 3_100);
    assert!(
        a.counters().rto_fires >= 1,
        "the dropped stream did not arm an RTO"
    );
    assert!(
        a.counters().retrans_segments >= 1,
        "the dropped stream was not retransmitted"
    );
    drop.set(false);
    let mut now = 3_100;
    let mut received = Vec::new();
    let mut buffer = vec![0u8; 65_536];
    while received.len() < data.len() {
        now += 1;
        a.poll(Instant::from_millis(now));
        b.poll(Instant::from_millis(now));
        drain(&mut b, sb, &mut received, &mut buffer);
        assert!(
            now < 90_000,
            "RTO stream stalled after the device recovered"
        );
    }
    assert_eq!(fnv1a(&received), fnv1a(&data));
    let stats = validate_trace(&link.trace(), &data, first_seq).expect("oracle");
    assert!(stats.segments >= 1);
}

#[test]
fn a_corrupted_reference_stream_is_detected() {
    let (link, mut a, mut b) = pair(StackConfig::default(), fast_link(10), 2_003);
    let (sa, sb) = handshake(&mut a, &mut b);
    let (_, first_seq, _) = a.test_tcp_window(sa).expect("window");
    let data = payload(2_003, 2_048);
    let mut now = 60;
    let received = send_stream(&mut a, &mut b, sa, sb, &data, &mut now, 2_003);
    assert_eq!(fnv1a(&received), fnv1a(&data));
    let mut reference = data.clone();
    reference[5] ^= 0xff;
    let error = validate_trace(&link.trace(), &reference, first_seq)
        .expect_err("the oracle must reject a corrupted reference");
    assert!(
        error.contains("payload mismatch"),
        "unexpected error: {error}"
    );
}
