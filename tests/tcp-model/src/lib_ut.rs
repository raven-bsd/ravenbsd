// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Randomized model/property tests and the current-stack edge adapters.

use std::collections::BTreeSet;

use raven_tcpip::{
    Errno, Instant, IpAddr, Ipv4Addr, Ipv4Config, RtoEstimator, SackScoreboard, SocketId, Stack,
    StackConfig,
};
use raven_tcpip_sim::{Endpoint, Link, LinkConfig};

use crate::congestion;
use crate::rng::Rng;
use crate::rto::{NaiveRto, RtoModel, zero_window_probe_delay_ms};
use crate::sack::SackedSet;
use crate::seq::{
    seq_distance, seq_ge, seq_gt, seq_le, seq_lt, seq_max, seq_min, seq_sub, tcp_rcvseqinit,
    tcp_sendseqinit,
};
use crate::state::{Event, State, output_flags, reference, state_name, transition};
use crate::window::{NaiveWindow, SendWindow};

const A: [u8; 4] = [192, 0, 2, 1];
const B: [u8; 4] = [192, 0, 2, 2];
const AMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x01];
const BMAC: [u8; 6] = [0x02, 0, 0, 0, 0, 0x02];

#[test]
fn model_sources_have_no_forbidden_donor_provenance() {
    let needle_a = ["Ne", "tBSD"].concat().to_ascii_lowercase();
    let needle_b = ["N", "R", "L"].concat().to_ascii_lowercase();
    let needle_c = ["tcp_", "sack.c"].concat().to_ascii_lowercase();
    let source_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = Vec::new();
    for entry in std::fs::read_dir(&source_dir).expect("read src") {
        let path = entry.expect("entry").path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("read source");
        let lower = text.to_ascii_lowercase();
        if lower.contains(&needle_a) || lower.contains(&needle_b) || lower.contains(&needle_c) {
            hits.push(
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    hits.sort();
    assert!(
        hits.is_empty(),
        "forbidden no-copy donor provenance remains in model sources: {hits:?}"
    );
}

fn signed_delta(a: u32, b: u32) -> i32 {
    let raw = i64::from(a) - i64::from(b);
    let wrapped = if raw > i64::from(i32::MAX) {
        raw - (1i64 << 32)
    } else if raw < i64::from(i32::MIN) {
        raw + (1i64 << 32)
    } else {
        raw
    };
    wrapped as i32
}

#[test]
fn sequence_arithmetic_matches_the_naive_reference_for_10k_pairs() {
    let mut rng = Rng::new(0x1000_0001);
    for _ in 0..10_000 {
        let a = rng.next_u32();
        let b = rng.next_u32();
        let delta = signed_delta(a, b);
        assert_eq!(seq_lt(a, b), delta < 0);
        assert_eq!(seq_le(a, b), delta <= 0);
        assert_eq!(seq_gt(a, b), delta > 0);
        assert_eq!(seq_ge(a, b), delta >= 0);
        assert_eq!(seq_distance(a, b), b.wrapping_sub(a));
        assert_eq!(seq_sub(a, b), delta);
        let minimum = if delta < 0 { a } else { b };
        let maximum = if delta > 0 { a } else { b };
        assert_eq!(seq_min(a, b), minimum);
        assert_eq!(seq_max(a, b), maximum);
    }
    for a in [0, 1, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff] {
        for b in [0, 1, 0x7fff_ffff, 0x8000_0000, 0xffff_ffff] {
            let delta = signed_delta(a, b);
            assert_eq!(seq_lt(a, b), delta < 0);
            assert_eq!(seq_gt(a, b), delta > 0);
        }
    }
    let send = tcp_sendseqinit(0x8000_0100);
    assert_eq!(send.snd_una, 0x8000_0100);
    assert_eq!(send.snd_nxt, send.snd_una);
    assert_eq!(send.snd_max, send.snd_una);
    assert_eq!(send.snd_up, send.snd_una);
    assert_eq!(send.snd_recover, send.snd_una);
    assert_eq!(tcp_rcvseqinit(0xffff_ffff), (0, 0));
}

#[test]
fn window_model_matches_the_naive_offset_reference_for_10k_operations() {
    let mut rng = Rng::new(0x1000_0002);
    let iss = 0x1000u32;
    let mss = 64u32;
    let cwnd = 8 * mss;
    let mut model = SendWindow::new(iss, mss, cwnd);
    let mut naive = NaiveWindow::new(mss, cwnd);
    assert_eq!(model.iss, iss);
    assert_eq!(model.mss, naive.mss);
    for step in 0..10_000 {
        match rng.below(6) {
            0 | 1 => {
                let seq = rng.below(10_000);
                let ack = rng.below(10_000);
                let wnd = rng.range(0, 65_535) as u16;
                let model_ok =
                    model.update_window(iss.wrapping_add(seq), iss.wrapping_add(ack), wnd);
                let naive_ok = naive.update_window(seq, ack, wnd);
                assert_eq!(model_ok, naive_ok, "window update at step {step}");
                assert_eq!(model.snd_wnd, naive.snd_wnd, "scaled window at step {step}");
                assert_eq!(model.max_snd_wnd, naive.max_snd_wnd);
            }
            2 => {
                let len = rng.below(500);
                let wnd = rng.below(65_536);
                let model_sent = model.send(len, wnd);
                let naive_sent = naive.send(len, wnd);
                assert_eq!(model_sent, naive_sent, "send at step {step}");
                assert_eq!(
                    seq_distance(iss, model.snd_nxt),
                    naive.snd_nxt,
                    "snd_nxt at step {step}"
                );
                assert_eq!(model.inflight(), naive.inflight());
                assert_eq!(model.usable(wnd), naive.usable(wnd));
            }
            3 => {
                let ack = rng.below(20_000);
                let model_acked = model.ack(iss.wrapping_add(ack));
                let naive_acked = naive.ack(ack);
                assert_eq!(model_acked, naive_acked, "ack at step {step}");
                assert_eq!(seq_distance(iss, model.snd_una), naive.snd_una);
            }
            4 => {
                let queued = rng.below(100);
                let model_probe = model.persist_probe(queued);
                let naive_probe = naive.persist_probe(queued);
                assert_eq!(model_probe, naive_probe, "persist at step {step}");
                assert_eq!(seq_distance(iss, model.snd_nxt), naive.snd_nxt);
            }
            _ => {
                assert!(model.valid_ack(model.snd_una));
            }
        }
    }
    assert_eq!(
        model.snd_una,
        iss.wrapping_add(seq_distance(iss, model.snd_una))
    );
}

#[test]
fn window_update_accepts_a_newer_ack_and_rejects_a_smaller_window() {
    let mut model = SendWindow::new(1_000, 1_200, 4_800);
    model.snd_wnd = 2_000;
    model.snd_wl1 = 100;
    model.snd_wl2 = 90;

    assert!(
        model.update_window(100, 95, 2_000),
        "same SEQ with a newer ACK must update the window"
    );
    assert_eq!(model.snd_wl2, 95);

    assert!(
        !model.update_window(100, 95, 1_000),
        "same SEQ and ACK with a smaller window must be ignored"
    );
    assert_eq!(model.snd_wnd, 2_000);

    assert!(
        model.update_window(100, 95, 2_500),
        "same SEQ and ACK with a larger window must update"
    );
    assert_eq!(model.snd_wnd, 2_500);

    assert!(
        !model.update_window(99, 200, 3_000),
        "an older SEQ must not update the window"
    );
    assert!(!model.window_is_newer(100, 95, 2_500));
    assert!(model.window_is_newer(100, 95, 2_600));
}

#[test]
fn state_model_matches_the_reference_table_and_covers_all_states() {
    let states = [
        State::Closed,
        State::Listen,
        State::SynSent,
        State::SynReceived,
        State::Established,
        State::CloseWait,
        State::FinWait1,
        State::Closing,
        State::LastAck,
        State::FinWait2,
        State::TimeWait,
    ];
    let events = [
        Event::PassiveOpen,
        Event::ActiveOpen,
        Event::ReceiveSyn,
        Event::ReceiveSynAck,
        Event::ReceiveAck,
        Event::ReceiveAckFin,
        Event::ReceiveFin,
        Event::ReceiveRst,
        Event::UserClose,
        Event::Timeout,
    ];
    let mut visited = BTreeSet::new();
    for state in states {
        assert!(!state_name(state).is_empty());
        let _ = output_flags(state);
        for event in events {
            let model = transition(state, event);
            let reference_transition = reference(state, event);
            assert_eq!(
                model, reference_transition,
                "state {state:?} event {event:?}"
            );
            visited.insert(model.next);
        }
    }
    assert_eq!(visited.len(), 11, "every FSM state must be reachable");

    let mut rng = Rng::new(0x1000_0003);
    let mut state = State::Closed;
    for step in 0..10_000 {
        let event = events[usize::try_from(rng.below(10)).unwrap_or(0)];
        let model = transition(state, event);
        let reference_transition = reference(state, event);
        assert_eq!(model, reference_transition, "walk step {step}");
        state = model.next;
    }
}

#[test]
fn rto_model_and_current_stack_respect_bounds_for_10k_operations() {
    let mut rng = Rng::new(0x1000_0004);
    let mut model = RtoModel::new(1_000.0, 200.0, 60_000.0);
    let mut naive = NaiveRto::new(1_000, 200, 60_000);
    let mut stack = RtoEstimator::new(1_000, 200, 60_000, 1);
    assert_eq!(zero_window_probe_delay_ms(1_000, 0), 1_000);
    assert_eq!(zero_window_probe_delay_ms(1_000, 3), 8_000);
    assert_eq!(zero_window_probe_delay_ms(1_000, 6), 60_000);
    assert_eq!(zero_window_probe_delay_ms(500, 0), 1_000);
    let mut divergences = 0usize;
    let mut first = String::new();
    for step in 0..10_000 {
        if rng.below(3) == 0 {
            model.timeout();
            naive.timeout();
            stack.timeout();
            assert!(model.rto_ms <= 60_000.0);
            assert!((200..=60_000).contains(&stack.rto_ms()));
            let base = model.rto_ms as u64;
            let delay = model.probe_delay_ms(base);
            assert_eq!(delay, zero_window_probe_delay_ms(base, model.backoff));
            assert!((1_000..=60_000).contains(&delay));
        } else {
            let rtt = u64::from(rng.range(1, 500));
            model.sample(rtt as f64);
            naive.sample(rtt);
            stack.sample(rtt);
            assert!((model.rto_ms - naive.rto_ms() as f64).abs() <= 1.0);
            let diff = (i64::try_from(stack.rto_ms()).unwrap_or(i64::MAX)
                - i64::try_from(naive.rto_ms()).unwrap_or(i64::MAX))
            .abs();
            if diff > 5 {
                divergences = divergences.saturating_add(1);
                if first.is_empty() {
                    first = format!(
                        "step {step} rtt={rtt} stack={} naive={}",
                        stack.rto_ms(),
                        naive.rto_ms()
                    );
                }
            }
            assert!((200..=60_000).contains(&stack.rto_ms()));
            assert!(stack.has_sample());
        }
    }
    println!("tcp-model: RTO 10k divergences={divergences} first={first}");
}

#[test]
fn sack_scoreboard_matches_the_byte_set_reference_and_reports_divergences() {
    let mut rng = Rng::new(0x1000_0005);
    let mut scoreboard = SackScoreboard::new(256, 0, 0, 64, 3, 1024);
    let mut reference = SackedSet::new(256);
    let mut structural = 0usize;
    let mut byte_count = 0usize;
    let mut predicates = 0usize;
    let mut first = String::new();
    for step in 0..10_000 {
        let start = i64::from(rng.range(0, 300)) - 32;
        let end = start + i64::from(rng.range(1, 80));
        scoreboard.insert_block(start, end);
        reference.insert(start, end);
        assert!(
            reference.invariants_hold(),
            "SACK reference invariant at step {step}"
        );
        if scoreboard.sacked != reference.runs() {
            structural = structural.saturating_add(1);
            if first.is_empty() {
                first = format!(
                    "step {step} sacked={:?} reference={:?}",
                    scoreboard.sacked,
                    reference.runs()
                );
            }
        }
        if scoreboard.gaps() != reference.gaps() {
            structural = structural.saturating_add(1);
            if first.is_empty() {
                first = format!("step {step} gaps");
            }
        }
        let point = rng.below(257);
        let product_bytes = scoreboard.sacked_bytes_above(point);
        let reference_bytes = reference.sacked_bytes_above(point);
        if product_bytes != reference_bytes {
            assert_eq!(
                product_bytes,
                reference_bytes + 1,
                "step {step}: unexpected byte-count delta at point {point}"
            );
            byte_count = byte_count.saturating_add(1);
            if first.is_empty() {
                first = format!("step {step} sacked bytes point={point}");
            }
        }
        if scoreboard.is_lost(point) != reference.is_lost(point, 64, 3) {
            assert_eq!(
                product_bytes,
                reference_bytes + 1,
                "step {step}: is_lost delta without a byte-count delta"
            );
            assert_eq!(
                reference_bytes, 128,
                "step {step}: is_lost delta away from the byte threshold"
            );
            predicates = predicates.saturating_add(1);
        }
        if scoreboard.pipe() != reference.pipe(0, 64, 3) {
            predicates = predicates.saturating_add(1);
        }
        if scoreboard.lost_ranges() != reference.lost_ranges(64, 3) {
            predicates = predicates.saturating_add(1);
        }
    }
    println!(
        "tcp-model: SACK 10k structural_divergences={structural} byte_count_divergences={byte_count} predicate_divergences={predicates} first={first}"
    );
    assert_eq!(
        structural, 0,
        "SACK ranges/gaps must match the byte-set reference: {first}"
    );
}

#[test]
fn congestion_full_feature_properties_for_10k_operations() {
    if let Err(error) = congestion::check(0x1000_0007, 10_000) {
        panic!("congestion property failure: {error}");
    }
}

fn address(octets: [u8; 4]) -> Ipv4Addr {
    Ipv4Addr::new(octets)
}

fn stack(endpoint: Endpoint, octets: [u8; 4], seed: u64) -> Stack<Endpoint> {
    let mut stack = Stack::new(endpoint, StackConfig::default(), seed, Instant::ZERO);
    stack
        .set_ipv4(Ipv4Config {
            addr: address(octets),
            prefix_len: 24,
            gateway: None,
        })
        .expect("set_ipv4");
    stack
}

fn pump<X, Y>(a: &mut Stack<X>, b: &mut Stack<Y>, from: i64, to: i64)
where
    X: raven_tcpip::Device,
    Y: raven_tcpip::Device,
{
    let mut time = from;
    while time <= to {
        let now = Instant::from_millis(time);
        a.poll(now);
        b.poll(now);
        time += 5;
    }
}

fn pair() -> (Stack<Endpoint>, Stack<Endpoint>, SocketId, SocketId) {
    let (client_endpoint, server_endpoint) = Link::new(LinkConfig::default(), 71);
    let mut client = stack(client_endpoint, A, 1);
    let mut server = stack(server_endpoint, B, 2);
    client.test_insert_neighbor(address(B), BMAC, Instant::ZERO, 60_000);
    server.test_insert_neighbor(address(A), AMAC, Instant::ZERO, 60_000);
    server.tcp_listen(80, 8).expect("listen");
    let connected = client
        .tcp_connect((IpAddr::V4(address(B)), 80), Some(40_000))
        .expect("connect");
    pump(&mut client, &mut server, 0, 200);
    let accepted = server
        .tcp_accept(SocketId(1))
        .expect("accept")
        .expect("connection");
    (client, server, connected, accepted)
}

fn drain(client: &mut Stack<Endpoint>, socket: SocketId) -> Vec<u8> {
    let mut received = Vec::new();
    let mut buffer = [0u8; 4096];
    loop {
        match client.recv(socket, &mut buffer) {
            Ok(0) | Err(Errno::EAGAIN) => break,
            Ok(count) => received.extend_from_slice(&buffer[..count]),
            Err(error) => panic!("recv: {error:?}"),
        }
    }
    received
}

#[test]
fn k252_zero_window_persist_against_the_current_stack() {
    let (mut client, mut server, connected, accepted) = pair();
    assert!(server.test_set_tcp_snd_wnd(accepted, 0));
    let data: Vec<u8> = (0..44).map(|index| (index % 251) as u8).collect();
    assert_eq!(server.send(accepted, &data), Ok(data.len()));
    assert_eq!(
        server.socket_info(accepted).expect("info").send_queue,
        data.len()
    );
    assert_eq!(client.recv_queue_len(connected), Ok(0));

    server.poll(Instant::from_millis(1_200));
    client.poll(Instant::from_millis(1_250));
    assert_eq!(
        client.recv_queue_len(connected),
        Ok(1),
        "the K252 persist probe must send exactly one byte"
    );
    client.poll(Instant::from_millis(1_300));
    server.poll(Instant::from_millis(1_350));
    client.poll(Instant::from_millis(1_410));
    assert_eq!(drain(&mut client, connected), data);
    println!(
        "tcp-model: K252 zero-window persist PASS probe=1 delivered=44 observed_first_probe<=1200ms model_min=1000ms"
    );
}

#[test]
fn k255_sack_cumulative_ack_and_range_cap_edges_are_reported() {
    let mut scoreboard = SackScoreboard::new(1_000, 0, 0, 100, 3, 32);
    scoreboard.insert_block(200, 300);
    assert_eq!(scoreboard.sacked, vec![(200, 300)]);
    scoreboard.high_ack = 400;
    let mut cumulative = SackedSet::new(1_000);
    cumulative.insert(200, 300);
    cumulative.ack(400);
    assert!(cumulative.runs().is_empty());
    println!(
        "tcp-model: K255 SACK cumulative ACK 400: sacked={:?} gaps={:?}; a cumulative-ACK trim would remove the below-ACK range (naive after ack {:?})",
        scoreboard.sacked,
        scoreboard.gaps(),
        cumulative.runs()
    );
    assert_eq!(scoreboard.sacked, vec![(200, 300)]);

    let mut capped = SackScoreboard::new(1_000, 0, 0, 100, 3, 1);
    capped.insert_block(100, 200);
    capped.insert_block(150, 250);
    println!(
        "tcp-model: K255 SACK range cap=1: sacked={:?}; the merge is dropped before normalize",
        capped.sacked
    );
    assert_eq!(capped.sacked, vec![(100, 200)]);

    let mut bytes = SackScoreboard::new(1_000, 0, 0, 64, 3, 64);
    bytes.insert_block(200, 300);
    let mut byte_reference = SackedSet::new(1_000);
    byte_reference.insert(200, 300);
    println!(
        "tcp-model: K255 SACK sacked_bytes_above(200): stack={} naive={}; the stack counts the byte at the point",
        bytes.sacked_bytes_above(200),
        byte_reference.sacked_bytes_above(200)
    );
    assert_eq!(bytes.sacked_bytes_above(200), 100);
    assert_eq!(byte_reference.sacked_bytes_above(200), 99);
}
