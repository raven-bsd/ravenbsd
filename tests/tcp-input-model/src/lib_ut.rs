// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Model, property and real-stack differential tests for the TCP input path.

use raven_abi::errno::Errno;
use raven_tcpip::{Instant, TcpState};
use raven_tcpip_wire::tcp::TcpFlags;

use crate::harness::{A, B, frame, handshake, handshake_at, pair, stack4, trace_link, trace_stack};
use crate::model::{Action, Connection, Flags, ResetDecision, Segment, State, seq_ge, seq_lt};
use crate::rng::Rng;

fn timestamp_options(tsval: u32, tsecr: u32) -> Vec<u8> {
    let mut options = vec![8u8, 10];
    options.extend_from_slice(&tsval.to_be_bytes());
    options.extend_from_slice(&tsecr.to_be_bytes());
    options
}

#[test]
fn model_syn_received_dispatch_matrix() {
    let mut connection = Connection::syn_received(1_000, 2_000);
    assert_eq!(
        connection.input(&Segment::new(2_001, 1_001, 65_535, Flags::syn()), 0),
        Action::SendAck
    );
    assert_eq!(connection.state, State::SynReceived);

    let mut connection = Connection::syn_received(1_000, 2_000);
    assert_eq!(
        connection.input(&Segment::new(2_000, 0, 65_535, Flags::syn()), 0),
        Action::SendSynAck
    );
    assert_eq!(connection.state, State::SynReceived);

    let mut connection = Connection::syn_received(1_000, 2_000);
    assert_eq!(
        connection.input(&Segment::new(1_999, 0, 65_535, Flags::syn()), 0),
        Action::DropWithReset
    );
    assert_eq!(connection.state, State::SynReceived);

    let mut connection = Connection::syn_received(1_000, 2_000);
    let final_ack = Segment::new(2_001, 1_001, 1_000, Flags::ack()).with_len(5);
    assert_eq!(connection.input(&final_ack, 0), Action::SendAck);
    assert_eq!(connection.state, State::Established);
    assert_eq!(connection.snd_una, 1_001);
    assert_eq!(connection.snd_wnd, 1_000);
    assert_eq!(connection.snd_wl1, 2_000);
    assert_eq!(connection.data_bytes, 5);

    let mut connection = Connection::syn_received(1_000, 2_000);
    assert_eq!(
        connection.input(&Segment::new(2_001, 1_000, 65_535, Flags::ack()), 0),
        Action::SendRst
    );
    assert_eq!(connection.state, State::SynReceived);

    let mut connection = Connection::syn_received(1_000, 2_000);
    assert_eq!(
        connection.input(&Segment::new(2_001, 0, 65_535, Flags::rst()), 0),
        Action::Close(Some(Errno::ECONNREFUSED))
    );
    assert_eq!(connection.state, State::Closed);

    let mut connection = Connection::syn_received(1_000, 2_000);
    assert_eq!(
        connection.input(&Segment::new(2_001 + 100_000, 0, 65_535, Flags::rst()), 0),
        Action::None
    );
    assert_eq!(connection.state, State::SynReceived);
    assert_eq!(connection.rst_ignored, 1);
}

#[test]
fn model_rst_disposition_uses_both_edges_and_the_window() {
    let mut connection = Connection::established(1_000, 2_000);
    connection.last_ack_sent = 2_000;
    assert_eq!(connection.reset_disposition(2_000), ResetDecision::Accept);
    assert_eq!(connection.reset_disposition(2_001), ResetDecision::Accept);
    assert_eq!(
        connection.reset_disposition(2_002),
        ResetDecision::Challenge
    );
    assert_eq!(
        connection.reset_disposition(2_001u32.wrapping_add(65_535)),
        ResetDecision::Ignore
    );
    assert_eq!(connection.reset_disposition(1_999), ResetDecision::Ignore);

    connection.rcv_wnd = 0;
    assert_eq!(connection.reset_disposition(2_000), ResetDecision::Accept);
    assert_eq!(connection.reset_disposition(2_001), ResetDecision::Accept);
    assert_eq!(connection.reset_disposition(2_002), ResetDecision::Ignore);
}

#[test]
fn model_paws_and_window_edges() {
    let mut connection = Connection::established(1_000, 2_000);
    connection.ts_recent = 500;
    connection.ts_recent_age_ms = 0;
    let stale = Segment::new(2_001, 1_001, 65_535, Flags::ack())
        .with_len(1)
        .with_timestamp(499);
    assert_eq!(connection.input(&stale, 1_000), Action::SendAck);
    assert_eq!(connection.paws_drops, 1);
    assert_eq!(connection.data_bytes, 0);

    let fresh = Segment::new(2_001, 1_001, 65_535, Flags::ack())
        .with_len(4)
        .with_timestamp(501);
    assert_eq!(connection.input(&fresh, 1_000), Action::None);
    assert_eq!(connection.data_bytes, 4);
    assert_eq!(connection.rcv_nxt, 2_005);

    let beyond =
        Segment::new(2_005u32.wrapping_add(65_535), 1_001, 65_535, Flags::ack()).with_len(8);
    assert_eq!(connection.input(&beyond, 1_000), Action::SendAck);
    assert_eq!(connection.data_bytes, 4);
    assert_eq!(connection.rcv_nxt, 2_005);
}

#[test]
fn model_sequence_wraparound() {
    let irs = u32::MAX - 5;
    let mut connection = Connection::syn_received(1_000, irs);
    assert_eq!(connection.rcv_nxt, irs.wrapping_add(1));

    let final_ack = Segment::new(connection.rcv_nxt, 1_001, 65_535, Flags::ack());
    assert_eq!(connection.input(&final_ack, 0), Action::SendAck);
    assert_eq!(connection.state, State::Established);

    let data = Segment::new(connection.rcv_nxt, 1_001, 65_535, Flags::ack()).with_len(8);
    assert_eq!(connection.input(&data, 0), Action::None);
    assert_eq!(connection.data_bytes, 8);
    assert_eq!(connection.rcv_nxt, 3);
}

#[test]
fn random_input_invariants() {
    let mut rng = Rng::new(0x3060_0001);
    for round in 0..40 {
        let iss = rng.next_u32();
        let irs = rng.next_u32();
        let mut connection = if round % 2 == 0 {
            Connection::syn_received(iss, irs)
        } else {
            Connection::established(iss, irs)
        };
        let base = connection.rcv_nxt;
        for step in 0..500 {
            let before = connection.state;
            let before_bytes = connection.data_bytes;
            let before_nxt = connection.rcv_nxt;
            let before_una = connection.snd_una;
            let before_max = connection.snd_max;
            let decision = connection.reset_disposition(base);
            let kind = rng.below(7);
            let segment = match kind {
                0 => Segment::new(base, iss.wrapping_add(1), 65_535, Flags::syn()),
                1 => Segment::new(base, iss.wrapping_add(1), 65_535, Flags::rst()),
                2 => Segment::new(
                    base.wrapping_add(rng.below(80_000)),
                    iss.wrapping_add(1),
                    65_535,
                    Flags::ack(),
                )
                .with_len(rng.below(64)),
                3 => {
                    Segment::new(base, iss.wrapping_add(1), 0, Flags::ack()).with_len(rng.below(8))
                }
                4 => Segment::new(base, iss.wrapping_add(1), 65_535, Flags::ack())
                    .with_timestamp(rng.next_u32()),
                5 => Segment::new(
                    base.wrapping_sub(rng.below(4)),
                    iss.wrapping_add(1),
                    65_535,
                    Flags::ack(),
                )
                .with_len(rng.below(8)),
                _ => Segment::new(base, iss.wrapping_add(1), 65_535, Flags::ack()),
            };
            let action = connection.input(&segment, 1_000);
            if before == State::SynReceived
                && segment.flags.syn
                && !segment.flags.ack
                && segment.seq != connection.irs
                && !seq_lt(segment.seq, connection.irs)
            {
                assert_eq!(
                    connection.state,
                    State::SynReceived,
                    "a non-land duplicate SYN closed the half-open at round {round} step {step}"
                );
            }
            if before == State::SynReceived
                && segment.flags.ack
                && !(segment.ack > before_una && segment.ack <= before_max)
            {
                assert_eq!(
                    connection.state,
                    State::SynReceived,
                    "an out-of-range ACK established the half-open at round {round} step {step}"
                );
            }
            if segment.flags.rst && decision != ResetDecision::Accept {
                assert_eq!(
                    connection.state, before,
                    "a non-accepted RST changed state at round {round} step {step}"
                );
            }
            if matches!(action, Action::Close(_)) {
                assert_eq!(connection.state, State::Closed);
            } else if before != State::Closed {
                assert_ne!(
                    connection.state,
                    State::Closed,
                    "state closed without a Close action at round {round} step {step}"
                );
            }
            assert!(
                connection.data_bytes >= before_bytes,
                "data_bytes went backwards at round {round} step {step}"
            );
            assert!(
                seq_ge(connection.rcv_nxt, before_nxt),
                "RCV.NXT went backwards at round {round} step {step}"
            );
        }
    }
}

#[test]
fn stack_listener_duplicate_syn_matches_model() {
    let mut b = stack4();
    let listener = b.tcp_listen(80, 4).expect("listen");
    let syn = frame(B, A, 40_000, 80, 100, 0, TcpFlags::SYN, 65_535, &[], &[]);
    b.test_inject_frame(&syn, Instant::ZERO);
    b.poll(Instant::ZERO);
    let half = b.test_listener_half_open(listener)[0];
    let iss = b.test_tcp_iss(half).expect("iss");
    let rcv_nxt = b.test_tcp_rcv_nxt(half).expect("rcv_nxt");

    b.test_clear_trace();
    let duplicate = frame(B, A, 40_000, 80, 100, 0, TcpFlags::SYN, 65_535, &[], &[]);
    b.test_inject_frame(&duplicate, Instant::from_millis(10));
    b.poll(Instant::from_millis(10));
    assert!(
        trace_stack(&b)
            .iter()
            .any(|(flags, _, _)| flags.syn() && flags.ack())
    );

    b.test_clear_trace();
    let other = frame(B, A, 40_000, 80, 1_000, 0, TcpFlags::SYN, 65_535, &[], &[]);
    b.test_inject_frame(&other, Instant::from_millis(20));
    b.poll(Instant::from_millis(20));
    assert_eq!(b.tcp_state(half), Ok(TcpState::SynReceived));
    assert!(
        trace_stack(&b)
            .iter()
            .any(|(flags, _, _)| flags.ack() && !flags.syn())
    );

    b.test_clear_trace();
    let land = frame(B, A, 40_000, 80, 99, 0, TcpFlags::SYN, 65_535, &[], &[]);
    b.test_inject_frame(&land, Instant::from_millis(30));
    b.poll(Instant::from_millis(30));
    assert_eq!(b.tcp_state(half), Ok(TcpState::SynReceived));
    let reset = trace_stack(&b)
        .into_iter()
        .find(|(flags, _, _)| flags.rst())
        .expect("LAND SYN reset");
    assert!(reset.0.ack());

    let final_ack = frame(
        B,
        A,
        40_000,
        80,
        rcv_nxt,
        iss.wrapping_add(1),
        TcpFlags::ACK,
        65_535,
        &[],
        &[],
    );
    b.test_inject_frame(&final_ack, Instant::from_millis(40));
    assert_eq!(b.tcp_state(half), Ok(TcpState::Established));
    assert!(b.tcp_accept(listener).expect("accept").is_some());
}

#[test]
fn stack_final_ack_payload_matches_model() {
    let mut b = stack4();
    let listener = b.tcp_listen(80, 4).expect("listen");
    let syn = frame(B, A, 40_000, 80, 100, 0, TcpFlags::SYN, 65_535, &[], &[]);
    b.test_inject_frame(&syn, Instant::ZERO);
    b.poll(Instant::ZERO);
    let half = b.test_listener_half_open(listener)[0];
    let iss = b.test_tcp_iss(half).expect("iss");
    let rcv_nxt = b.test_tcp_rcv_nxt(half).expect("rcv_nxt");

    let mut model = Connection::syn_received(iss, 100);
    let ack = frame(
        B,
        A,
        40_000,
        80,
        rcv_nxt,
        iss.wrapping_add(1),
        TcpFlags::ACK,
        65_535,
        &[],
        b"hello",
    );
    let model_action = model.input(
        &Segment::new(rcv_nxt, iss.wrapping_add(1), 65_535, Flags::ack()).with_len(5),
        0,
    );
    b.test_inject_frame(&ack, Instant::from_millis(10));
    b.poll(Instant::from_millis(10));
    assert_eq!(model_action, Action::SendAck);
    assert_eq!(model.state, State::Established);
    assert_eq!(model.data_bytes, 5);
    assert_eq!(b.tcp_state(half), Ok(TcpState::Established));
    let accepted = b
        .tcp_accept(listener)
        .expect("accept")
        .expect("established");
    let mut buffer = [0u8; 8];
    assert_eq!(b.recv(accepted, &mut buffer).expect("recv"), 5);
    assert_eq!(&buffer[..5], b"hello");
}

#[test]
fn stack_bad_ack_in_syn_received_matches_model() {
    let mut b = stack4();
    let listener = b.tcp_listen(80, 4).expect("listen");
    let syn = frame(B, A, 40_000, 80, 100, 0, TcpFlags::SYN, 65_535, &[], &[]);
    b.test_inject_frame(&syn, Instant::ZERO);
    b.poll(Instant::ZERO);
    let half = b.test_listener_half_open(listener)[0];
    let iss = b.test_tcp_iss(half).expect("iss");
    let rcv_nxt = b.test_tcp_rcv_nxt(half).expect("rcv_nxt");
    let mut model = Connection::syn_received(iss, 100);
    let action = model.input(&Segment::new(rcv_nxt, iss, 65_535, Flags::ack()), 0);
    assert_eq!(action, Action::SendRst);
    b.test_clear_trace();
    let bad = frame(
        B,
        A,
        40_000,
        80,
        rcv_nxt,
        iss,
        TcpFlags::ACK,
        65_535,
        &[],
        &[],
    );
    b.test_inject_frame(&bad, Instant::from_millis(10));
    b.poll(Instant::from_millis(10));
    assert_eq!(b.tcp_state(half), Ok(TcpState::SynReceived));
    assert!(trace_stack(&b).iter().any(|(flags, _, _)| flags.rst()));
}

#[test]
fn stack_rst_validation_matches_model() {
    let (_link, mut a, mut b) = pair();
    let (socket, _accepted) = handshake(&mut a, &mut b);
    let rcv_nxt = a.test_tcp_rcv_nxt(socket).expect("rcv_nxt");
    let mut model = Connection::established(1, rcv_nxt.wrapping_sub(1));
    model.rcv_nxt = rcv_nxt;
    model.last_ack_sent = rcv_nxt;
    assert_eq!(model.reset_disposition(rcv_nxt), ResetDecision::Accept);
    assert_eq!(
        model.reset_disposition(rcv_nxt.wrapping_add(100_000)),
        ResetDecision::Ignore
    );

    let outside = frame(
        B,
        A,
        80,
        40_000,
        rcv_nxt.wrapping_add(100_000),
        1,
        TcpFlags::RST,
        0,
        &[],
        &[],
    );
    a.test_inject_frame(&outside, Instant::from_millis(210));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Established));

    let inside = frame(B, A, 80, 40_000, rcv_nxt, 1, TcpFlags::RST, 0, &[], &[]);
    a.test_inject_frame(&inside, Instant::from_millis(220));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Closed));
    assert_eq!(a.tcp_error(socket), Ok(Some(Errno::ECONNRESET)));
}

#[test]
fn stack_paws_stale_timestamp_matches_model() {
    let (_link, mut a, mut b) = pair();
    let (_sa, sb) = handshake_at(&mut a, &mut b, 1_000);
    let ts_recent = b.test_tcp_options_info(sb).expect("options").3;
    let old = ts_recent.wrapping_sub(1);
    let rcv_nxt = b.test_tcp_rcv_nxt(sb).expect("rcv_nxt");
    let ack = b.test_tcp_window(sb).expect("window").2;
    let options = timestamp_options(old, ts_recent);

    let mut model = Connection::established(1, rcv_nxt.wrapping_sub(1));
    model.rcv_nxt = rcv_nxt;
    model.last_ack_sent = rcv_nxt;
    model.ts_recent = ts_recent;
    model.ts_recent_age_ms = 1_000;
    let action = model.input(
        &Segment::new(rcv_nxt, ack, 65_535, Flags::ack())
            .with_len(1)
            .with_timestamp(old),
        1_210,
    );
    assert_eq!(action, Action::SendAck);
    assert_eq!(model.data_bytes, 0);

    let stale = frame(
        A,
        B,
        40_000,
        80,
        rcv_nxt,
        ack,
        TcpFlags::ACK,
        65_535,
        &options,
        b"x",
    );
    b.test_inject_frame(&stale, Instant::from_millis(1_210));
    let paws = b.test_tcp_options_info(sb).expect("options").7;
    assert_eq!(paws, 1);
    let mut buffer = [0u8; 1];
    assert_eq!(b.recv(sb, &mut buffer), Err(Errno::EAGAIN));
}

#[test]
fn stack_sequence_wraparound_matches_model() {
    let mut b = stack4();
    let listener = b.tcp_listen(80, 4).expect("listen");
    let irs = u32::MAX - 5;
    let syn = frame(B, A, 40_000, 80, irs, 0, TcpFlags::SYN, 65_535, &[], &[]);
    b.test_inject_frame(&syn, Instant::ZERO);
    b.poll(Instant::ZERO);
    let half = b.test_listener_half_open(listener)[0];
    let iss = b.test_tcp_iss(half).expect("iss");
    let rcv_nxt = b.test_tcp_rcv_nxt(half).expect("rcv_nxt");
    assert_eq!(rcv_nxt, irs.wrapping_add(1));

    let ack = frame(
        B,
        A,
        40_000,
        80,
        rcv_nxt,
        iss.wrapping_add(1),
        TcpFlags::ACK,
        65_535,
        &[],
        &[],
    );
    b.test_inject_frame(&ack, Instant::from_millis(10));
    assert_eq!(b.tcp_state(half), Ok(TcpState::Established));

    let mut model = Connection::syn_received(iss, irs);
    assert_eq!(
        model.input(
            &Segment::new(rcv_nxt, iss.wrapping_add(1), 65_535, Flags::ack()),
            0
        ),
        Action::SendAck
    );
    let data = frame(
        B,
        A,
        40_000,
        80,
        rcv_nxt,
        iss.wrapping_add(1),
        TcpFlags::ACK,
        65_535,
        &[],
        b"wrapdata",
    );
    assert_eq!(
        model.input(
            &Segment::new(rcv_nxt, iss.wrapping_add(1), 65_535, Flags::ack()).with_len(8),
            0
        ),
        Action::None
    );
    b.test_inject_frame(&data, Instant::from_millis(20));
    b.poll(Instant::from_millis(20));
    let expected = rcv_nxt.wrapping_add(8);
    assert_eq!(model.rcv_nxt, expected);
    assert_eq!(b.test_tcp_rcv_nxt(half), Some(expected));
    let accepted = b
        .tcp_accept(listener)
        .expect("accept")
        .expect("established");
    let mut buffer = [0u8; 16];
    assert_eq!(b.recv(accepted, &mut buffer).expect("recv"), 8);
    assert_eq!(&buffer[..8], b"wrapdata");
}

#[test]
fn stack_syn_in_established_matches_model() {
    let (link, mut a, mut b) = pair();
    let (socket, _accepted) = handshake(&mut a, &mut b);
    let start = link.trace().entries().len();
    let mut model = Connection::established(1, a.test_tcp_rcv_nxt(socket).unwrap().wrapping_sub(1));
    model.rcv_nxt = a.test_tcp_rcv_nxt(socket).expect("rcv_nxt");
    assert_eq!(
        model.input(&Segment::new(model.rcv_nxt, 1, 65_535, Flags::syn()), 0),
        Action::Challenge
    );
    assert_eq!(model.state, State::Established);

    let rcv_nxt = a.test_tcp_rcv_nxt(socket).expect("rcv_nxt");
    let syn = frame(
        B,
        A,
        80,
        40_000,
        rcv_nxt,
        1,
        TcpFlags::SYN,
        65_535,
        &[],
        &[],
    );
    a.test_inject_frame(&syn, Instant::from_millis(210));
    a.poll(Instant::from_millis(210));
    assert_eq!(a.tcp_state(socket), Ok(TcpState::Established));
    assert!(
        trace_link(&link, start)
            .iter()
            .any(|(flags, _, _)| flags.ack() && !flags.syn())
    );
}

#[test]
fn stack_out_of_window_data_matches_model() {
    let (_link, mut a, mut b) = pair();
    let (socket, _accepted) = handshake(&mut a, &mut b);
    let rcv_nxt = a.test_tcp_rcv_nxt(socket).expect("rcv_nxt");
    let (_wnd, _, snd_nxt) = a.test_tcp_window(socket).expect("window");
    let mut model = Connection::established(1, rcv_nxt.wrapping_sub(1));
    model.rcv_nxt = rcv_nxt;
    let beyond = rcv_nxt.wrapping_add(100_000);
    assert_eq!(
        model.input(
            &Segment::new(beyond, snd_nxt, 65_535, Flags::ack()).with_len(4),
            0
        ),
        Action::SendAck
    );
    let frame = frame(
        B,
        A,
        80,
        40_000,
        beyond,
        snd_nxt,
        TcpFlags::ACK,
        65_535,
        &[],
        b"far!",
    );
    a.test_inject_frame(&frame, Instant::from_millis(210));
    let mut buffer = [0u8; 4];
    assert_eq!(a.recv(socket, &mut buffer), Err(Errno::EAGAIN));
}
