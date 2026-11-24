// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public K331 RACK ACK/retransmit checks.

use raven_tcpip::{
    FastAck, ProcessAck, RACK_HAS_FIN, RACK_SACK_PASSED, RackRetransmit, RetransmitError, TH_ACK,
};

fn filled() -> (RackRetransmit, [u32; 4]) {
    let mut model = RackRetransmit::new(1_000);
    model.set_base(1_000);
    let ids = [
        model.send(1_000, 2_000, 1_000),
        model.send(2_000, 3_000, 1_010),
        model.send(3_000, 4_000, 1_020),
        model.send(4_000, 5_000, 1_030),
    ];
    model.pace_max_segs = 1_000;
    (model, ids)
}

#[test]
fn three_duplicate_acks_select_and_send_the_oldest_segment() {
    let (mut model, ids) = filled();
    assert_eq!(model.rack_strike_dupack(1_000, 0, 2_000), None);
    assert_eq!(model.rack_strike_dupack(1_000, 0, 2_000), None);
    let selected = model.rack_strike_dupack(1_000, 0, 2_000);
    assert_eq!(selected, Some(ids[0]));
    let tx = model
        .rack_fast_rsm_output(selected.expect("selection"), 1_000, false, 3_000, 40)
        .expect("retransmit");
    assert_eq!(tx.seq, 1_000);
    assert_eq!(tx.len, 1_000);
    assert_eq!(tx.flags & TH_ACK, TH_ACK);
    assert_eq!(model.map.find_id(ids[0]).expect("entry").rtr_cnt, 2);
}

#[test]
fn process_ack_advances_the_map_and_exits_recovery() {
    let (mut model, _) = filled();
    model.in_fast_recovery = true;
    model.snd_recover = 5_000;
    model.stack.tp.snd_cwnd = 10_000;
    model.stack.tp.snd_ssthresh = 5_000;
    let result = model.rack_process_ack(5_000, 65_535, 0, &[], 0, 2_000);
    let ProcessAck::Acked(outcome) = result else {
        panic!("expected an advancing ACK, got {result:?}");
    };
    assert_eq!(outcome.acked, 4_000);
    assert!(outcome.post_recovery);
    assert_eq!(model.stack.tp.snd_una, 5_000);
    assert!(model.map.segments.is_empty());
    assert!(!model.in_fast_recovery);
}

#[test]
fn the_public_flags_keep_the_donor_values() {
    assert_eq!(RACK_HAS_FIN, 0x40);
    assert_eq!(RACK_SACK_PASSED, 0x10);
    let mut model = RackRetransmit::new(1_000);
    model.set_base(1_000);
    let id = model.send(1_000, 2_000, 1_000);
    let ack = FastAck {
        ack: 2_000,
        window: 4_000,
        tlen: 0,
        sacks: &[],
        sendbuf_avail: 100,
        now_us: 2_000,
        timestamp_ok: true,
    };
    assert!(model.rack_fastack(ack));
    assert_eq!(model.stack.tp.snd_una, 2_000);
    assert_eq!(
        model.rack_fast_rsm_output(id, 1_000, false, 3_000, 40),
        Err(RetransmitError::NoSegment),
        "the cum-ACK removed the entry from the send map"
    );
}
