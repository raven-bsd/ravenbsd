// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Self-tests for the simulated link, trace and virtual-time driver.

use raven_tcpip::{
    ChecksumOffload, Device, Duration, Instant, Medium, Stack, StackConfig, TxError,
};
use raven_tcpip_sim::{Direction, Link, LinkConfig, Sim, Trace};

fn send(
    endpoint: &mut raven_tcpip_sim::Endpoint,
    now: Instant,
    payload: &[u8],
) -> Result<(), TxError> {
    endpoint.transmit(now, payload.len(), &mut |buffer| {
        buffer.copy_from_slice(payload);
    })
}

fn receive(endpoint: &mut raven_tcpip_sim::Endpoint, now: Instant) -> Option<Vec<u8>> {
    let mut received = None;
    if endpoint.receive(now, &mut |frame| received = Some(frame.to_vec())) {
        received
    } else {
        None
    }
}

fn drain(endpoint: &mut raven_tcpip_sim::Endpoint, now: Instant) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    while let Some(frame) = receive(endpoint, now) {
        frames.push(frame);
    }
    frames
}

#[test]
fn frame_crosses_with_half_rtt_delay() {
    let (mut a, mut b) = Link::new(LinkConfig::default(), 1);
    let link = a.link();
    send(&mut a, Instant::ZERO, b"ping").expect("send");
    assert_eq!(
        link.next_delivery(Instant::ZERO),
        Some(Instant::from_millis(50))
    );
    assert_eq!(receive(&mut b, Instant::from_millis(49)), None);
    assert_eq!(
        receive(&mut b, Instant::from_millis(50)),
        Some(b"ping".to_vec())
    );
}

#[test]
fn loss_100_delivers_nothing() {
    let config = LinkConfig {
        loss_percent: 100.0,
        ..LinkConfig::default()
    };
    let (mut a, mut b) = Link::new(config, 1);
    for index in 0..5u8 {
        send(&mut a, Instant::ZERO, &[index]).expect("send");
    }
    assert!(drain(&mut b, Instant::from_millis(1000)).is_empty());
}

#[test]
fn loss_0_delivers_everything() {
    let (mut a, mut b) = Link::new(LinkConfig::default(), 1);
    for index in 0..5u8 {
        send(&mut a, Instant::ZERO, &[index]).expect("send");
    }
    let frames = drain(&mut b, Instant::from_millis(1000));
    assert_eq!(frames.len(), 5);
}

#[test]
fn same_seed_hits_the_same_losses() {
    let config = LinkConfig {
        loss_percent: 50.0,
        ..LinkConfig::default()
    };
    let run = |seed| {
        let (mut a, mut b) = Link::new(config, seed);
        for index in 0..20u8 {
            send(&mut a, Instant::ZERO, &[index]).expect("send");
        }
        drain(&mut b, Instant::from_millis(1000))
            .into_iter()
            .map(|frame| frame[0])
            .collect::<Vec<_>>()
    };
    assert_eq!(run(77), run(77));
}

#[test]
fn same_seed_produces_identical_trace_text() {
    let config = LinkConfig {
        corrupt_percent: 50.0,
        reorder_percent: 20.0,
        ..LinkConfig::default()
    };
    let run = |seed| {
        let (mut a, mut b) = Link::new(config, seed);
        for index in 0..8u8 {
            send(&mut a, Instant::from_millis(i64::from(index)), b"frame").expect("send");
        }
        drain(&mut b, Instant::from_millis(1000));
        a.link().trace_text()
    };
    assert_eq!(run(5), run(5));
}

#[test]
fn different_seeds_produce_different_traces() {
    let config = LinkConfig {
        corrupt_percent: 100.0,
        ..LinkConfig::default()
    };
    let run = |seed| {
        let (mut a, mut b) = Link::new(config, seed);
        send(&mut a, Instant::ZERO, b"corrupt me").expect("send");
        drain(&mut b, Instant::from_millis(100));
        a.link().trace_text()
    };
    assert_ne!(run(1), run(2));
}

#[test]
fn reorder_actually_reorders_and_never_duplicates() {
    let config = LinkConfig {
        rtt_ms: 100,
        reorder_percent: 100.0,
        ..LinkConfig::default()
    };
    let (mut a, mut b) = Link::new(config, 9);
    let sent: Vec<u8> = (0..16).collect();
    for value in &sent {
        send(&mut a, Instant::ZERO, &[*value]).expect("send");
    }
    let received: Vec<u8> = drain(&mut b, Instant::from_millis(1000))
        .into_iter()
        .map(|frame| frame[0])
        .collect();
    assert_eq!(received.len(), sent.len(), "reorder must not duplicate");
    assert_ne!(
        received, sent,
        "100% reorder must change the order for this seed"
    );
}

#[test]
fn duplicate_100_doubles_every_frame() {
    let config = LinkConfig {
        duplicate_percent: 100.0,
        ..LinkConfig::default()
    };
    let (mut a, mut b) = Link::new(config, 1);
    for index in 0..5u8 {
        send(&mut a, Instant::ZERO, &[index]).expect("send");
    }
    assert_eq!(drain(&mut b, Instant::from_millis(1000)).len(), 10);
}

#[test]
fn frame_larger_than_mtu_is_dropped_and_counted() {
    let config = LinkConfig {
        mtu: 8,
        ..LinkConfig::default()
    };
    let (mut a, mut b) = Link::new(config, 1);
    let link = a.link();
    assert_eq!(
        send(&mut a, Instant::ZERO, b"123456789"),
        Err(TxError::TooLong)
    );
    assert_eq!(link.drops(), 1);
    assert!(drain(&mut b, Instant::from_millis(1000)).is_empty());
}

#[test]
fn queue_overflow_drops_the_newest_and_counts_it() {
    let config = LinkConfig {
        queue_bytes: 10,
        ..LinkConfig::default()
    };
    let (mut a, mut b) = Link::new(config, 1);
    let link = a.link();
    send(&mut a, Instant::ZERO, b"123456").expect("first");
    send(&mut a, Instant::ZERO, b"abcdef").expect("second");
    assert_eq!(link.drops(), 1);
    let frames = drain(&mut b, Instant::from_millis(1000));
    assert_eq!(frames, vec![b"123456".to_vec()]);
}

#[test]
fn bandwidth_spaces_frames() {
    let config = LinkConfig {
        rtt_ms: 100,
        bandwidth_bps: Some(8_000),
        ..LinkConfig::default()
    };
    let (mut a, mut b) = Link::new(config, 1);
    let link = a.link();
    send(&mut a, Instant::ZERO, &[0u8; 100]).expect("first");
    send(&mut a, Instant::from_millis(1), &[0u8; 100]).expect("second");
    assert_eq!(
        link.next_delivery(Instant::ZERO),
        Some(Instant::from_millis(50))
    );
    assert_eq!(
        receive(&mut b, Instant::from_millis(50)),
        Some(vec![0u8; 100])
    );
    // The first frame occupies the wire for 100 ms; the second departs at 100 ms and arrives at 150 ms.
    assert_eq!(
        link.next_delivery(Instant::from_millis(51)),
        Some(Instant::from_millis(150))
    );
}

#[test]
fn run_until_times_out_with_a_non_empty_trace() {
    let (a, b) = Link::new(LinkConfig::default(), 1);
    let link = a.link();
    let mut a = a;
    send(&mut a, Instant::ZERO, b"timeout").expect("send");
    let mut sim = Sim::new(
        vec![
            Stack::new(a, StackConfig::default(), 1, Instant::ZERO),
            Stack::new(b, StackConfig::default(), 2, Instant::ZERO),
        ],
        vec![link],
        Instant::ZERO,
    );
    let timeout = sim
        .run_until(|_| false, Duration::from_millis(200))
        .expect_err("must time out");
    assert!(!timeout.trace.is_empty());
    assert!(timeout.trace.contains("A>B"));
}

#[test]
fn trace_assert_matches_exact_text() {
    let (mut a, mut b) = Link::new(LinkConfig::default(), 1);
    send(&mut a, Instant::ZERO, b"ping").expect("send");
    let _ = drain(&mut b, Instant::from_millis(50));
    let trace = a.link().trace();
    assert_eq!(trace.to_text(), "A>B t=0 len=4 70696e67\n");
    trace
        .assert_matches("A>B t=0 len=4 70696e67\n")
        .expect("exact match");
}

#[test]
fn trace_mismatch_reports_expected_and_actual() {
    let mut trace = Trace::new();
    trace.record(Direction::BtoA, Instant::from_millis(7), b"x");
    let error = trace.assert_matches("wrong").expect_err("mismatch");
    assert_eq!(error.expected, "wrong");
    assert!(error.actual.contains("B>A t=7"));
}

#[test]
fn set_config_changes_loss_mid_test() {
    let (mut a, mut b) = Link::new(LinkConfig::default(), 1);
    let link = a.link();
    send(&mut a, Instant::ZERO, b"before").expect("send");
    assert_eq!(drain(&mut b, Instant::from_millis(100)).len(), 1);
    link.set_config(LinkConfig {
        loss_percent: 100.0,
        ..LinkConfig::default()
    });
    send(&mut a, Instant::from_millis(200), b"after").expect("send");
    assert!(drain(&mut b, Instant::from_millis(500)).is_empty());
}

#[test]
fn endpoint_capabilities_follow_the_config() {
    let config = LinkConfig {
        mtu: 900,
        ..LinkConfig::default()
    };
    let (a, b) = Link::new(config, 1);
    let caps = a.capabilities();
    assert_eq!(caps.medium, Medium::Ethernet);
    assert_eq!(caps.mtu, 900);
    assert_eq!(caps.checksum_offload, ChecksumOffload::None);
    assert!(caps.hardware_addr.is_some());
    assert_ne!(caps.hardware_addr, b.capabilities().hardware_addr);
    assert!(a.is_a());
}

#[test]
fn next_delivery_is_none_when_empty() {
    let (a, _b) = Link::new(LinkConfig::default(), 1);
    assert_eq!(a.link().next_delivery(Instant::ZERO), None);
}

#[test]
fn corruption_changes_bytes_but_not_length() {
    let config = LinkConfig {
        corrupt_percent: 100.0,
        ..LinkConfig::default()
    };
    let (mut a, mut b) = Link::new(config, 3);
    let original = b"abcdefgh";
    send(&mut a, Instant::ZERO, original).expect("send");
    let received = receive(&mut b, Instant::from_millis(100)).expect("delivered");
    assert_eq!(received.len(), original.len());
    assert_ne!(received, original);
}

#[test]
fn sim_drives_two_stacks_to_the_deadline() {
    let (a, b) = Link::new(LinkConfig::default(), 1);
    let link = a.link();
    let mut sim = Sim::new(
        vec![
            Stack::new(a, StackConfig::default(), 1, Instant::ZERO),
            Stack::new(b, StackConfig::default(), 2, Instant::ZERO),
        ],
        vec![link],
        Instant::ZERO,
    );
    let timeout = sim
        .run_until(|_| false, Duration::from_millis(100))
        .expect_err("no event completes the predicate");
    assert_eq!(timeout.now, Instant::from_millis(100));
}

#[test]
fn sim_advances_to_an_armed_stack_timer() {
    let (a, b) = Link::new(LinkConfig::default(), 1);
    let link = a.link();
    let mut stack_a = Stack::new(a, StackConfig::default(), 1, Instant::ZERO);
    stack_a
        .set_timer(Instant::from_millis(50))
        .expect("set_timer");
    let stack_b = Stack::new(b, StackConfig::default(), 2, Instant::ZERO);
    let mut sim = Sim::new(vec![stack_a, stack_b], vec![link], Instant::ZERO);
    sim.run_until(
        |sim| sim.now() >= Instant::from_millis(50),
        Duration::from_millis(500),
    )
    .expect("timer reached");
    assert_eq!(sim.now(), Instant::from_millis(50));
}

fn fnv1a(text: &str) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in text.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[test]
fn determinism_hash_is_stable_across_runs() {
    let config = LinkConfig {
        loss_percent: 2.0,
        duplicate_percent: 5.0,
        reorder_percent: 5.0,
        corrupt_percent: 5.0,
        ..LinkConfig::default()
    };
    let run = |seed| {
        let (mut a, mut b) = Link::new(config, seed);
        for index in 0..32u8 {
            send(&mut a, Instant::from_millis(i64::from(index)), &[index; 8]).expect("send");
        }
        for time in 0..2_000 {
            let _ = drain(&mut b, Instant::from_millis(time));
        }
        a.link().trace_text()
    };
    let first = run(2026);
    let second = run(2026);
    assert_eq!(first, second);
    eprintln!("trace fnv1a = {:016x}", fnv1a(&first));
}

#[test]
fn trace_clear_removes_entries() {
    let mut trace = Trace::new();
    trace.record(Direction::AtoB, Instant::ZERO, b"x");
    assert_eq!(trace.entries().len(), 1);
    trace.clear();
    assert!(trace.entries().is_empty());
    assert!(trace.to_text().is_empty());
}
