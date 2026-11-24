// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Reference-model, stream-hash and real-stack window tests.

use raven_tcpip::Instant;

use crate::harness::{ack_frame, drain, handshake, pair_with, pump, trace_link};
use crate::model::{StreamHash, WindowModel, hash_bytes};
use crate::rng::Rng;

fn pattern(len: usize) -> Vec<u8> {
    (0..len)
        .map(|index| u8::try_from(index % 251).unwrap_or(0))
        .collect()
}

#[test]
fn reference_window_floor_and_update_ordering() {
    let mut model = WindowModel::new(4_096, 1_460, 0);
    assert_eq!(model.advertised_window(), 4_096);
    model.queued = 2_048;
    assert_eq!(model.advertised_window(), 2_048);
    model.queued = 2_049;
    assert_eq!(
        model.advertised_window(),
        0,
        "below the MSS/half-buffer floor"
    );
    model.queued = 4_096;
    assert_eq!(model.advertised_window(), 0);
    model.queued = 0;
    assert_eq!(model.advertised_window(), 4_096);

    model.snd_wl1 = 100;
    model.snd_wl2 = 50;
    model.snd_wnd = 1_000;
    assert!(!model.update_window(100, 50, 500), "stale smaller window");
    assert!(
        model.update_window(100, 50, 1_500),
        "larger same-point window"
    );
    assert!(model.update_window(101, 50, 10), "newer sequence");
    assert!(!model.update_window(101, 50, 5), "stale same-point window");
    assert_eq!(model.snd_wnd, 10);
    assert_eq!(model.max_snd_wnd, 1_500);
}

#[test]
fn sws_decision_matches_freebsd_output_rule() {
    let mut model = WindowModel::new(4_096, 1_460, 0);
    model.max_snd_wnd = 4_096;
    assert!(
        !model.should_send(200, 0, 300, false, false),
        "small window, more queued"
    );
    assert!(
        model.should_send(300, 0, 300, false, false),
        "idle full queue"
    );
    assert!(
        model.should_send(1_460, 1_000, 5_000, false, false),
        "full MSS"
    );
    assert!(
        model.should_send(2_048, 1_000, 5_000, false, false),
        "half the max window"
    );
    assert!(
        model.should_send(1, 1_000, 5_000, false, true),
        "forced probe"
    );
    assert!(model.should_send(10, 1_000, 1_010, true, false), "NODELAY");
}

#[test]
fn stream_hash_distinguishes_loss_and_reorder() {
    assert_ne!(hash_bytes(b"ab"), hash_bytes(b"ba"));
    assert_ne!(hash_bytes(b"ab"), hash_bytes(b"a"));
    assert_ne!(hash_bytes(b"abcabc"), hash_bytes(b"abc"));
    let mut streaming = StreamHash::new();
    streaming.update(b"a");
    streaming.update(b"bc");
    assert_eq!(streaming.value(), hash_bytes(b"abc"));
}

#[test]
fn receive_window_shrinks_to_zero_and_matches_model() {
    let (_link, mut a, mut b) = pair_with(4_096);
    let (sa, sb) = handshake(&mut a, &mut b);
    let payload = pattern(8_192);
    assert_eq!(a.send(sa, &payload).expect("send"), payload.len());
    pump(&mut a, &mut b, 250, 10, 120);

    let queued = b.recv_queue_len(sb).expect("recv queue");
    assert_eq!(queued, 4_096, "the receiver accepted one full buffer");
    let (snd_wnd, _, _) = a.test_tcp_window(sa).expect("window");
    assert_eq!(snd_wnd, 0, "the sender learned the zero window");

    let mut model = WindowModel::new(4_096, 1_460, 0);
    model.snd_wnd = 4_096;
    assert_eq!(model.send(4_096), 4_096);
    assert_eq!(model.accept(&payload[..4_096]), 4_096);
    assert_eq!(model.advertised_window(), 0);
}

#[test]
fn zero_window_probe_fires_and_the_stream_reopens() {
    let (_link, mut a, mut b) = pair_with(4_096);
    let (sa, sb) = handshake(&mut a, &mut b);
    let baseline = b.test_tcp_rcv_nxt(sb).expect("rcv_nxt");
    let payload = pattern(8_192);
    assert_eq!(a.send(sa, &payload).expect("send"), payload.len());
    pump(&mut a, &mut b, 250, 10, 120);
    assert_eq!(a.test_tcp_window(sa).expect("window").0, 0);

    let probes_before = a.test_tcp_zero_window_probes(sa).unwrap_or(0);
    pump(&mut a, &mut b, 1_500, 100, 60);
    let probes_after = a.test_tcp_zero_window_probes(sa).unwrap_or(0);
    assert!(
        probes_after > probes_before,
        "persist probes {probes_before} -> {probes_after}"
    );

    let mut received = Vec::new();
    let mut now = 7_500;
    for _ in 0..40 {
        received.extend(drain(&mut b, sb, 1_024));
        pump(&mut a, &mut b, now, 10, 30);
        now += 300;
        if received.len() == payload.len() {
            break;
        }
    }
    assert_eq!(received.len(), payload.len(), "the whole stream arrives");
    assert_eq!(hash_bytes(&received), hash_bytes(&payload), "stream hash");
    let rcv_nxt = b.test_tcp_rcv_nxt(sb).expect("rcv_nxt");
    assert_eq!(
        rcv_nxt.wrapping_sub(baseline),
        u32::try_from(payload.len()).unwrap_or(u32::MAX),
        "every sequence number is delivered once"
    );

    let mut model = WindowModel::new(4_096, 1_460, 0);
    model.snd_wnd = 4_096;
    model.send(4_096);
    model.snd_wnd = 0;
    model.accept(&payload[..4_096]);
    assert!(model.probe_due(4_096));
    assert!(model.probe(4_096));
    assert_eq!(model.probes, 1);
    for chunk in payload[4_096..].chunks(4_096) {
        model.drain(4_096);
        assert!(model.free() == 4_096, "the application drained the buffer");
        model.accept(chunk);
        assert_eq!(model.hash.value(), hash_bytes(&payload[..model.delivered]));
    }
    assert_eq!(model.hash.value(), hash_bytes(&payload));
}

#[test]
fn window_update_reopens_the_sender() {
    let (_link, mut a, mut b) = pair_with(4_096);
    let (sa, sb) = handshake(&mut a, &mut b);
    let payload = pattern(8_192);
    assert_eq!(a.send(sa, &payload).expect("send"), payload.len());
    pump(&mut a, &mut b, 250, 10, 120);
    assert_eq!(a.test_tcp_window(sa).expect("window").0, 0);

    let drained = drain(&mut b, sb, 1_024);
    assert_eq!(drained.len(), 4_096);
    pump(&mut a, &mut b, 1_500, 10, 1);
    let reopened = a.test_tcp_window(sa).expect("window").0;
    assert!(reopened > 0, "the drained receiver advertised a window");
    pump(&mut a, &mut b, 1_520, 10, 120);
    assert!(
        b.recv_queue_len(sb).expect("queue") > 0,
        "the sender moved after the window update"
    );

    let mut model = WindowModel::new(4_096, 1_460, 0);
    model.queued = 4_096;
    assert_eq!(model.advertised_window(), 0);
    assert_eq!(model.drain(1_024), 1_024);
    assert_eq!(model.advertised_window(), 0, "still below the floor");
    assert_eq!(model.drain(3_072), 3_072);
    assert_eq!(model.advertised_window(), 4_096, "a full drain reopens");
}

#[test]
fn duplicate_ack_does_not_move_the_window() {
    let (_link, mut a, mut b) = pair_with(4_096);
    let (sa, sb) = handshake(&mut a, &mut b);
    assert_eq!(a.send(sa, b"x").expect("send"), 1);
    pump(&mut a, &mut b, 250, 10, 20);
    let (snd_wnd, snd_una, _) = a.test_tcp_window(sa).expect("window");
    let rcv_nxt = b.test_tcp_rcv_nxt(sb).expect("rcv_nxt");
    let frame = ack_frame(rcv_nxt, snd_una, 10);
    a.test_inject_frame(&frame, Instant::from_millis(500));
    a.poll(Instant::from_millis(500));
    let after = a.test_tcp_window(sa).expect("window");
    assert_eq!(
        after.0, snd_wnd,
        "a duplicate ACK must not shrink the window"
    );
    assert_eq!(after.1, snd_una, "a duplicate ACK must not move SND.UNA");

    let mut model = WindowModel::new(4_096, 1_460, 0);
    model.snd_wnd = snd_wnd;
    model.snd_una = snd_una;
    model.snd_max = snd_una;
    model.snd_wl1 = rcv_nxt;
    model.snd_wl2 = snd_una;
    assert!(model.is_duplicate_ack(snd_una, 0));
    model.ack(rcv_nxt, snd_una, 10);
    assert_eq!(model.snd_wnd, snd_wnd);
    assert_eq!(model.snd_una, snd_una);
}

#[test]
fn sws_withholds_a_partial_segment_until_the_window_fits_the_queue() {
    let (link, mut a, mut b) = pair_with(4_096);
    let (sa, _sb) = handshake(&mut a, &mut b);

    let start = link.trace().entries().len();
    assert_eq!(a.send(sa, b"tiny").expect("send"), 4);
    a.poll(Instant::from_millis(300));
    assert!(
        trace_link(&link, start)
            .iter()
            .any(|(flags, _, _, len)| flags.ack() && *len == 4),
        "an idle small write is sent"
    );
    pump(&mut a, &mut b, 310, 10, 20);

    assert!(a.test_set_tcp_snd_wnd(sa, 201));
    let start = link.trace().entries().len();
    assert_eq!(a.send(sa, &[0x11; 300]).expect("send"), 300);
    a.poll(Instant::from_millis(600));
    assert!(
        !trace_link(&link, start)
            .iter()
            .any(|(_, _, _, len)| *len > 0),
        "a 201-byte window must not carry a 201-byte partial segment of 300"
    );

    assert!(a.test_set_tcp_snd_wnd(sa, 301));
    let start = link.trace().entries().len();
    a.send(sa, b"x").expect("send");
    a.poll(Instant::from_millis(610));
    assert!(
        trace_link(&link, start)
            .iter()
            .any(|(_, _, _, len)| *len > 0),
        "the segment is sent when the window covers the queue"
    );
}

#[test]
fn randomized_send_drain_stream_matches_the_reference_hash() {
    let (_link, mut a, mut b) = pair_with(4_096);
    let (sa, sb) = handshake(&mut a, &mut b);
    let baseline = b.test_tcp_rcv_nxt(sb).expect("rcv_nxt");
    let payload = pattern(32_768);
    assert_eq!(a.send(sa, &payload).expect("send"), payload.len());

    let mut rng = Rng::new(0x3100_0001);
    let mut received = Vec::new();
    let mut saw_zero = false;
    let mut now = 300;
    for _ in 0..300 {
        let chunk = 512 + usize::try_from(rng.below(3_584)).unwrap_or(0);
        received.extend(drain(&mut b, sb, chunk));
        pump(&mut a, &mut b, now, 10, 20);
        now += 200;
        if a.test_tcp_window(sa).expect("window").0 == 0 {
            saw_zero = true;
        }
        let queued = b.recv_queue_len(sb).expect("queue");
        assert!(queued <= 4_096, "receive queue exceeds the cap");
        let rcv_nxt = b.test_tcp_rcv_nxt(sb).expect("rcv_nxt");
        assert_eq!(
            rcv_nxt.wrapping_sub(baseline),
            u32::try_from(received.len() + queued).unwrap_or(u32::MAX),
            "RCV.NXT moved by exactly the delivered plus queued bytes"
        );
        if received.len() == payload.len() && a.send_queue_len(sa).unwrap_or(0) == 0 {
            break;
        }
    }

    assert_eq!(received.len(), payload.len(), "no bytes lost or duplicated");
    assert_eq!(hash_bytes(&received), hash_bytes(&payload), "stream hash");
    let model_hash = {
        let mut model = WindowModel::new(4_096, 1_460, 0);
        for chunk in payload.chunks(4_096) {
            model.drain(4_096);
            model.accept(chunk);
        }
        assert_eq!(model.delivered, payload.len());
        model.hash.value()
    };
    assert_eq!(model_hash, hash_bytes(&payload));
    assert!(saw_zero, "the exercise must reach a zero window");
}
