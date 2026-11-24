// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Reference, real-stack and randomized tests for the K312 timestamp/PAWS model.

use crate::harness::{self, Established};
use crate::model::{self, Connection, Segment};
use crate::rng::Rng;
use raven_tcpip::StackConfig;

const BASE_MS: i64 = 1_000_000;
const BASE_TS: u32 = 1_000_000;
/// Absolute FreeBSD `TCP_PAWS_IDLE` (24 days), independent of the model constant so a
/// mutation of the model's idle window is caught by the oracle.
const FREEBSD_PAWS_IDLE_MS: u64 = 24 * 24 * 60 * 60 * 1000;

fn config(rx_queue_bytes: usize) -> StackConfig {
    StackConfig {
        rx_queue_bytes,
        ..StackConfig::default()
    }
}

fn establish(irs: u32, rx_queue_bytes: usize) -> Established {
    harness::establish(config(rx_queue_bytes), irs, BASE_TS, BASE_MS)
}

fn rcv_nxt(established: &Established) -> u32 {
    established
        .stack
        .test_tcp_rcv_nxt(established.socket)
        .expect("rcv_nxt")
}

fn options(established: &Established) -> (bool, u32, usize, u32) {
    harness::options_state(established)
}

#[test]
fn timestamps_are_negotiated_and_echoed() {
    let mut established = establish(10_000, 65_536);
    let (timestamps, ts_recent, paws, _) = options(&established);
    assert!(timestamps, "timestamps must be negotiated");
    // The syncache promotes with the SYN's TSval; the final ACK's timestamp is not
    // installed as ts_recent (RFC 7323 records the timestamp that established the state).
    assert_eq!(ts_recent, BASE_TS);
    assert_eq!(established.synack_tsecr, BASE_TS);
    assert!(established.synack_ts > 0);
    assert_eq!(paws, 0);

    let seq = rcv_nxt(&established);
    let tsval = ts_recent + 10;
    harness::inject(
        &mut established,
        seq,
        tsval,
        ts_recent,
        65_535,
        b"ts",
        BASE_MS + 10,
    );
    assert_eq!(options(&established).1, tsval);
    assert_eq!(harness::drain(&mut established), b"ts");
}

#[test]
fn paws_rejects_stale_and_accepts_new() {
    let mut established = establish(10_000, 65_536);
    let rcv = rcv_nxt(&established);
    let (_, ts_recent, _, _) = options(&established);
    let now = (BASE_MS + 10) as u64;
    let mut model = Connection::established(rcv, ts_recent, (BASE_MS + 1) as u64);

    let stale_ts = ts_recent.wrapping_sub(1);
    harness::inject(
        &mut established,
        rcv,
        stale_ts,
        ts_recent,
        65_535,
        b"stale",
        BASE_MS + 10,
    );
    let stale = model.receive(now, &Segment::new(rcv, b"stale").with_timestamp(stale_ts));
    assert!(stale.paws_drop);
    assert_eq!(model.paws_drops, 1);
    assert_eq!(options(&established).2, 1);
    assert_eq!(rcv_nxt(&established), rcv);
    assert!(harness::drain(&mut established).is_empty());

    let fresh_ts = ts_recent.wrapping_add(1);
    harness::inject(
        &mut established,
        rcv,
        fresh_ts,
        ts_recent,
        65_535,
        b"fresh",
        BASE_MS + 11,
    );
    let fresh = model.receive(
        (BASE_MS + 11) as u64,
        &Segment::new(rcv, b"fresh").with_timestamp(fresh_ts),
    );
    assert_eq!(fresh.accepted, 5);
    assert_eq!(model.rcv_nxt, rcv.wrapping_add(5));
    assert_eq!(rcv_nxt(&established), model.rcv_nxt);
    assert_eq!(harness::drain(&mut established), b"fresh");
    assert_eq!(options(&established).1, fresh_ts);
    assert_eq!(model.ts_recent, fresh_ts);
}

#[test]
fn reference_uses_the_freebsd_24_day_boundary() {
    let mut at_boundary = Connection::established(100, 1_000, 0);
    let stale = Segment::new(100, b"x").with_timestamp(999);
    let outcome = at_boundary.receive(FREEBSD_PAWS_IDLE_MS, &stale);
    assert!(outcome.paws_drop, "FreeBSD drops at exactly 24 days");
    assert_eq!(at_boundary.paws_drops, 1);
    assert_eq!(at_boundary.rcv_nxt, 100);

    let mut after_boundary = Connection::established(100, 1_000, 0);
    let outcome = after_boundary.receive(FREEBSD_PAWS_IDLE_MS + 1, &stale);
    assert!(outcome.aged_out, "FreeBSD ages ts_recent after 24 days");
    assert!(!outcome.paws_drop);
    assert_eq!(after_boundary.paws_drops, 0);
    assert_eq!(after_boundary.ts_recent, 999);
    assert_eq!(after_boundary.rcv_nxt, 101);
}

#[test]
fn product_accepts_a_stale_timestamp_at_exactly_24_days() {
    let mut established = establish(10_000, 65_536);
    let rcv = rcv_nxt(&established);
    let (_, ts_recent, _, _) = options(&established);
    let now = BASE_MS + 1 + i64::try_from(FREEBSD_PAWS_IDLE_MS).expect("24 days fits i64");
    harness::inject(
        &mut established,
        rcv,
        ts_recent.wrapping_sub(1),
        ts_recent,
        65_535,
        b"x",
        now,
    );
    // FreeBSD/NetBSD drop at exactly 24 days (`age > 24d` fails), while the product's
    // `age < 24d` drop condition accepts. This test records the exact-boundary finding;
    // it does not modify product code.
    assert_eq!(options(&established).2, 0);
    assert_eq!(rcv_nxt(&established), rcv.wrapping_add(1));
    assert_eq!(harness::drain(&mut established), b"x");
}

#[test]
fn sequence_wrap_and_old_duplicate_match_the_reference() {
    let irs = u32::MAX - 5;
    let mut established = establish(irs, 65_536);
    let rcv = rcv_nxt(&established);
    assert_eq!(rcv, irs.wrapping_add(1));
    let (_, ts_recent, _, _) = options(&established);
    let mut model = Connection::established(rcv, ts_recent, (BASE_MS + 1) as u64);

    let payload = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    let fresh_ts = ts_recent.wrapping_add(1);
    harness::inject(
        &mut established,
        rcv,
        fresh_ts,
        ts_recent,
        65_535,
        &payload,
        BASE_MS + 10,
    );
    let outcome = model.receive(
        (BASE_MS + 10) as u64,
        &Segment::new(rcv, &payload).with_timestamp(fresh_ts),
    );
    assert_eq!(outcome.accepted, payload.len());
    assert_eq!(model.rcv_nxt, 3, "the sequence crosses u32::MAX");
    assert_eq!(rcv_nxt(&established), model.rcv_nxt);
    assert_eq!(harness::drain(&mut established), payload);

    let old_seq = u32::MAX - 2;
    let stale_ts = ts_recent.wrapping_sub(1);
    harness::inject(
        &mut established,
        old_seq,
        stale_ts,
        ts_recent,
        65_535,
        b"old",
        BASE_MS + 11,
    );
    let old = model.receive(
        (BASE_MS + 11) as u64,
        &Segment::new(old_seq, b"old").with_timestamp(stale_ts),
    );
    assert!(old.paws_drop);
    assert_eq!(rcv_nxt(&established), model.rcv_nxt);
    assert_eq!(options(&established).2, model.paws_drops);
}

#[test]
fn zero_window_timestamp_edges() {
    let mut established = establish(20_000, 2_048);
    let (_, mut ts_recent, _, _) = options(&established);
    let mut rcv = rcv_nxt(&established);

    let mut queued = 0usize;
    for step in 0..16u32 {
        if queued >= 2_048 {
            break;
        }
        let payload = vec![u8::try_from(step).unwrap_or(0); 512];
        let tsval = ts_recent.wrapping_add(1);
        harness::inject(
            &mut established,
            rcv,
            tsval,
            ts_recent,
            65_535,
            &payload,
            BASE_MS + 20 + i64::from(step),
        );
        ts_recent = tsval;
        rcv = rcv_nxt(&established);
        queued = established
            .stack
            .recv_queue_len(established.socket)
            .expect("queue length");
    }
    assert_eq!(
        queued, 2_048,
        "the receive queue must reach the 2048-byte cap"
    );

    // A segment at the right window edge is past the zero window; it forces an ACK and
    // does not consume queue space.
    let edge_ts = ts_recent.wrapping_add(1);
    harness::inject(
        &mut established,
        rcv,
        edge_ts,
        ts_recent,
        65_535,
        b"w",
        BASE_MS + 40,
    );
    ts_recent = edge_ts;

    let mut model = Connection::established(rcv, ts_recent, (BASE_MS + 40) as u64);
    model.rcv_wnd = 0;
    model.ts_recent_age_ms = (BASE_MS + 40) as u64;

    let stale_ts = ts_recent.wrapping_sub(1);
    let probe_seq = rcv.wrapping_sub(1);
    let queued_before = queued;
    harness::inject(
        &mut established,
        probe_seq,
        stale_ts,
        ts_recent,
        65_535,
        b"x",
        BASE_MS + 40,
    );
    let stale = model.receive(
        (BASE_MS + 40) as u64,
        &Segment::new(probe_seq, b"x").with_timestamp(stale_ts),
    );
    assert!(stale.paws_drop);
    assert_eq!(options(&established).2, 1);
    assert_eq!(rcv_nxt(&established), rcv);
    assert_eq!(
        established
            .stack
            .recv_queue_len(established.socket)
            .expect("queue"),
        queued_before
    );

    let fresh_ts = ts_recent.wrapping_add(1);
    harness::inject(
        &mut established,
        probe_seq,
        fresh_ts,
        ts_recent,
        65_535,
        b"y",
        BASE_MS + 41,
    );
    let fresh = model.receive(
        (BASE_MS + 41) as u64,
        &Segment::new(probe_seq, b"y").with_timestamp(fresh_ts),
    );
    assert_eq!(fresh.accepted, 0);
    assert_eq!(model.ts_recent, fresh_ts);
    assert_eq!(options(&established).1, fresh_ts);
    assert_eq!(rcv_nxt(&established), rcv);
    assert_eq!(
        established
            .stack
            .recv_queue_len(established.socket)
            .expect("queue"),
        queued_before
    );
}

#[test]
fn ten_thousand_randomized_sequences_match_the_reference() {
    const SEED: u64 = 0x0312_7a77_5e01_0001;
    const STEPS: usize = 10_000;

    let mut established = establish(u32::MAX - 50_000, 1 << 20);
    let (_, mut ts_recent, _, _) = options(&established);
    let mut rcv = rcv_nxt(&established);
    let mut model = Connection::established(rcv, ts_recent, (BASE_MS + 1) as u64);
    let mut product_stream = Vec::new();
    let mut rng = Rng::new(SEED);
    let mut now_ms = BASE_MS + 1;
    let mut saw_wrap = false;

    for step in 0..STEPS {
        now_ms += i64::from(rng.below(50));
        let mode = rng.below(10);
        let (seq, payload, tsval) = if mode < 7 {
            let length = 1 + rng.below(64);
            let payload = (0..length)
                .map(|index| u8::try_from((step as u32 + index) & 0xff).unwrap_or(0))
                .collect::<Vec<_>>();
            (rcv, payload, ts_recent.wrapping_add(1 + rng.below(10_000)))
        } else if mode < 9 {
            let offset = 1 + rng.below(1_024);
            let length = offset + rng.below(32);
            let payload = vec![u8::try_from(step & 0xff).unwrap_or(0); length as usize];
            (
                rcv.wrapping_sub(offset),
                payload,
                ts_recent.wrapping_sub(1 + rng.below(10_000)),
            )
        } else {
            (
                rcv,
                Vec::new(),
                ts_recent.wrapping_add(1 + rng.below(10_000)),
            )
        };

        harness::inject(
            &mut established,
            seq,
            tsval,
            ts_recent,
            65_535,
            &payload,
            now_ms,
        );
        let received = harness::drain(&mut established);
        product_stream.extend_from_slice(&received);
        let outcome = model.receive(
            now_ms as u64,
            &Segment::new(seq, &payload).with_timestamp(tsval),
        );
        assert_eq!(
            outcome.accepted,
            received.len(),
            "step {step}: accepted byte count differs (seq {seq}, len {}, tsval {tsval})",
            payload.len()
        );
        let state = options(&established);
        assert_eq!(
            state.1, model.ts_recent,
            "step {step}: ts_recent differs (seq {seq}, tsval {tsval}, mode {mode})"
        );
        assert_eq!(
            state.2, model.paws_drops,
            "step {step}: PAWS counter differs"
        );
        assert_eq!(
            rcv_nxt(&established),
            model.rcv_nxt,
            "step {step}: rcv_nxt differs (seq {seq}, len {})",
            payload.len()
        );
        assert_eq!(
            model::fnv1a(&product_stream),
            model.stream_hash(),
            "step {step}: stream hash differs"
        );
        if outcome.accepted > 0 && seq.wrapping_add(payload.len() as u32) < seq {
            saw_wrap = true;
        }
        if model.rcv_nxt < rcv {
            saw_wrap = true;
        }
        rcv = model.rcv_nxt;
        ts_recent = model.ts_recent;
    }

    assert!(saw_wrap, "the randomized sequence must cross u32::MAX");
    assert_eq!(model.stream().len(), product_stream.len());
    assert_eq!(model::fnv1a(&product_stream), model.stream_hash());
    assert_eq!(SEED, 0x0312_7a77_5e01_0001);
}
