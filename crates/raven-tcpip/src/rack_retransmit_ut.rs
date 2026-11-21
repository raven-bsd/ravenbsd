// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the K331 RACK ACK/send-map/retransmit donor group.

use super::*;

fn base_model() -> RackRetransmit {
    let mut model = RackRetransmit::new(1_000);
    model.set_base(1_000);
    model.stack.tp.snd_cwnd = 10_000;
    model.cwnd_to_use = 10_000;
    model
}

fn fill(model: &mut RackRetransmit) -> [u32; 3] {
    [
        model.send(1_000, 2_000, 1_000),
        model.send(2_000, 3_000, 1_100),
        model.send(3_000, 4_000, 1_200),
    ]
}

#[test]
fn rack_find_lowest_rsm_skips_acked_entries() {
    let mut model = base_model();
    let ids = fill(&mut model);
    assert_eq!(model.rack_find_lowest_rsm(), Some(ids[0]));
    assert!(model.map.mark_acked(ids[0], &mut model.sacked));
    assert_eq!(model.rack_find_lowest_rsm(), Some(ids[1]));
    assert!(model.map.mark_acked(ids[1], &mut model.sacked));
    assert_eq!(model.rack_find_lowest_rsm(), Some(ids[2]));
    assert!(model.map.mark_acked(ids[2], &mut model.sacked));
    assert_eq!(model.rack_find_lowest_rsm(), None);
}

#[test]
fn rack_find_high_nonack_skips_acked_and_fin_segments() {
    let mut model = base_model();
    let ids = fill(&mut model);
    assert_eq!(model.rack_find_high_nonack(ids[2]), Some(ids[2]));
    assert!(model.map.mark_acked(ids[2], &mut model.sacked));
    if let Some(entry) = model.map.find_id_mut(ids[1]) {
        entry.flags |= RACK_HAS_FIN;
    }
    assert_eq!(
        model.rack_find_high_nonack(ids[2]),
        Some(ids[0]),
        "an acked tail and a FIN-bearing middle must be skipped"
    );
    assert_eq!(model.rack_find_high_nonack(ids[0]), Some(ids[0]));
    assert_eq!(model.rack_find_high_nonack(999), None);
}

#[test]
fn rack_update_rsm_counts_retransmits_and_moves_time_order() {
    let mut model = base_model();
    let ids = fill(&mut model);
    model.rack_update_rsm(ids[0], 2_000, RACK_SENT_FP, 1_000);
    let entry = model.map.find_id(ids[0]).expect("entry");
    assert_eq!(entry.rtr_cnt, 2);
    assert_eq!(entry.act_rxt_cnt, 1);
    assert_eq!(entry.dupack, 0);
    assert_eq!(entry.rtr_bytes, 1_000);
    assert_eq!(entry.tim_lastsent[1], 2_000);
    assert_eq!(entry.bas, 1);
    assert_eq!(entry.fas, 3_000);
    assert_eq!(model.holes_rxt, 1_000);
    assert_eq!(model.map.tmap.last(), Some(&ids[0]), "moved to the tail");

    model.rack_update_rsm(ids[0], 2_100, RACK_SENT_FP, 1_000);
    model.rack_update_rsm(ids[0], 2_200, RACK_SENT_FP, 1_000);
    let entry = model.map.find_id(ids[0]).expect("entry");
    assert_eq!(entry.rtr_cnt, RACK_NUM_OF_RETRANS as u8);
    assert_ne!(entry.flags & RACK_OVERMAX, 0);
    assert_eq!(entry.tim_lastsent[2], 2_200);

    if let Some(entry) = model.map.find_id_mut(ids[0]) {
        entry.flags |= RACK_WAS_LOST | RACK_ACKED | RACK_MUST_RXT | RACK_SACK_PASSED;
    }
    model.considered_lost = 1_000;
    model.sacked = 1_000;
    model.must_retran = true;
    model.out_at_rto = 1_000;
    model.snd_max_at_rto = 4_000;
    model.rack_update_rsm(ids[0], 2_300, RACK_SENT_FP, 1_000);
    let entry = model.map.find_id(ids[0]).expect("entry");
    assert_eq!(entry.flags & RACK_WAS_LOST, 0);
    assert_eq!(entry.flags & RACK_ACKED, 0);
    assert_eq!(entry.flags & RACK_MUST_RXT, 0);
    assert_eq!(entry.flags & RACK_SACK_PASSED, 0);
    assert_ne!(entry.flags & RACK_WAS_SACKPASS, 0);
    assert_eq!(model.considered_lost, 0);
    assert_eq!(model.sacked, 0);
    assert_eq!(model.out_at_rto, 0);
    assert!(model.must_retran, "the RTO ceiling was not reached");
    assert_eq!(model.recovery_rxt_cnt, 1_000);
}

#[test]
fn rack_need_set_test_advances_the_goodput_window() {
    let mut model = base_model();
    let ids = fill(&mut model);
    model.stack.tp.t_flags |= TF_GPUTINPROG;
    model.gput_seq = 1_500;
    model.gput_ts = 100;
    model.gput_ack = 1_500;
    model.act_rcv_time = 5_000;
    model.rack_need_set_test(ids[0], 2_000, RACK_USE_END);
    assert_eq!(model.gput_seq, 2_000);
    assert_eq!(model.gput_ts, 5_000);
    assert_eq!(model.gput_ack, 12_000);
    assert_eq!(model.gp_output_ts, 1_100);
    assert!(!model.app_limited_needs_set);
    assert_eq!(model.gp_logs, 1);
    assert_eq!(model.gp_marks_tended, 1);

    // The sequence at snd_max has not been sent: stay app-limited.
    model.gput_seq = 3_000;
    model.gput_ts = 200;
    model.gput_ack = 3_000;
    model.rack_need_set_test(ids[2], 4_000, RACK_USE_END);
    assert_eq!(model.gput_seq, 4_000);
    assert!(model.app_limited_needs_set);
    assert_eq!(model.gp_output_ts, 1_200);

    // A retransmitted entry has an ambiguous timestamp and must not move the window.
    let retransmitted = fill(&mut model)[0];
    model.rack_update_rsm(retransmitted, 2_000, RACK_SENT_FP, 1_000);
    model.gput_seq = 1_500;
    model.gput_ts = 300;
    model.gput_ack = 1_500;
    let before = (model.gput_seq, model.gput_ts, model.gput_ack);
    model.rack_need_set_test(retransmitted, 4_000, RACK_USE_END);
    assert_eq!((model.gput_seq, model.gput_ts, model.gput_ack), before);
}

#[test]
fn rack_check_reorder_ack_undoes_spurious_recovery() {
    let mut model = base_model();
    let id = model.send(1_000, 2_000, 1_000);
    model.rack_update_rsm(id, 2_000, RACK_SENT_FP, 1_000);
    if let Some(entry) = model.map.find_id_mut(id) {
        entry.flags |= RACK_WAS_SACKPASS;
    }
    model.in_fast_recovery = true;
    model.stack.tp.t_srtt = 800;
    model.recovery_rxt_cnt = 1_000;
    model.ssthresh_at_erec = 5_000;
    model.might_revert = true;
    model.retran_during_recovery = 1_000;
    assert!(model.rack_check_reorder_ack(id, 2_000, 2_300, true));
    assert_eq!(model.reorder_ts, 2_300);
    assert_eq!(model.recovery_rxt_cnt, 0);
    assert_eq!(model.stack.tp.snd_ssthresh, 5_000);
    assert!(!model.in_fast_recovery);
    assert!(!model.might_revert);
    assert_eq!(model.retran_during_recovery, 0);
    assert_eq!(model.prr_logs, 2);

    // Too slow to be a reordered ACK.
    let mut late = base_model();
    let id = late.send(1_000, 2_000, 1_000);
    late.rack_update_rsm(id, 2_000, RACK_SENT_FP, 1_000);
    if let Some(entry) = late.map.find_id_mut(id) {
        entry.flags |= RACK_WAS_SACKPASS;
    }
    late.in_fast_recovery = true;
    late.stack.tp.t_srtt = 800;
    assert!(!late.rack_check_reorder_ack(id, 2_000, 3_000, true));

    // Without the exit permission the retransmit count is decremented but recovery stays.
    let mut held = base_model();
    let id = held.send(1_000, 2_000, 1_000);
    held.rack_update_rsm(id, 2_000, RACK_SENT_FP, 1_000);
    if let Some(entry) = held.map.find_id_mut(id) {
        entry.flags |= RACK_WAS_SACKPASS;
    }
    held.in_fast_recovery = true;
    held.stack.tp.t_srtt = 800;
    held.recovery_rxt_cnt = 1_000;
    assert!(held.rack_check_reorder_ack(id, 2_000, 2_300, false));
    assert_eq!(held.recovery_rxt_cnt, 0);
    assert!(held.in_fast_recovery);

    // The guard set: no recovery flag, wrong retransmit count, or no SACK-pass flag.
    let mut idle = base_model();
    let id = idle.send(1_000, 2_000, 1_000);
    idle.rack_update_rsm(id, 2_000, RACK_SENT_FP, 1_000);
    if let Some(entry) = idle.map.find_id_mut(id) {
        entry.flags |= RACK_WAS_SACKPASS;
    }
    idle.stack.tp.t_srtt = 800;
    assert!(!idle.rack_check_reorder_ack(id, 2_000, 2_300, true));
    idle.in_fast_recovery = true;
    if let Some(entry) = idle.map.find_id_mut(id) {
        entry.rtr_cnt = 1;
    }
    assert!(!idle.rack_check_reorder_ack(id, 2_000, 2_300, true));
    if let Some(entry) = idle.map.find_id_mut(id) {
        entry.rtr_cnt = 2;
        entry.flags &= !RACK_WAS_SACKPASS;
    }
    assert!(!idle.rack_check_reorder_ack(id, 2_000, 2_300, true));
}

#[test]
fn rack_update_entry_splits_partial_and_reports_overhang() {
    let mut model = RackRetransmit::new(1_000);
    let id = model.send(1_000, 3_000, 1_000);
    let result = model.rack_update_entry(id, 2_000, 500, RACK_SENT_FP, 1_000);
    assert_eq!(result.seq_out, 0);
    assert_eq!(result.remaining, 0);
    let tail = result.split.expect("split tail");
    let head = model.map.find_id(id).expect("head");
    assert_eq!((head.start, head.end), (1_000, 1_500));
    assert_eq!(head.rtr_cnt, 2);
    let tail = model.map.find_id(tail).expect("tail");
    assert_eq!((tail.start, tail.end), (1_500, 3_000));
    assert_eq!(
        tail.rtr_cnt, 1,
        "the tail keeps the original transmit count"
    );

    let mut full = RackRetransmit::new(1_000);
    let id = full.send(1_000, 3_000, 1_000);
    let result = full.rack_update_entry(id, 2_000, 2_000, RACK_SENT_FP, 1_000);
    assert_eq!(
        result,
        EntryResult {
            seq_out: 0,
            remaining: 0,
            split: None,
        }
    );
    assert_eq!(full.map.find_id(id).expect("entry").rtr_cnt, 2);

    let mut overhang = RackRetransmit::new(1_000);
    let id = overhang.send(1_000, 2_000, 1_000);
    let result = overhang.rack_update_entry(id, 2_000, 1_500, RACK_SENT_FP, 1_000);
    assert_eq!(result.seq_out, 2_000);
    assert_eq!(result.remaining, 500);
    assert_eq!(result.split, None);
}

#[test]
fn rack_update_rtt_tracks_the_minimum_and_rejects_spurious_samples() {
    let mut model = base_model();
    let id = model.send(1_000, 2_000, 1_000);
    assert_eq!(
        model.rack_update_rtt(id, None, 1_500, CUM_ACKED, 2_000),
        Some(500)
    );
    assert_eq!(model.rack_min_rtt, 500);
    assert_eq!(model.t_rttlow, 500);
    assert_eq!(model.srtt, 4_000);
    assert_eq!(model.rack_tmit_time, 1_000);
    assert_eq!(model.rack_rtt, 500);

    assert!(model.map.mark_acked(id, &mut model.sacked));
    assert_eq!(
        model.rack_update_rtt(id, None, 1_600, CUM_ACKED, 2_000),
        None,
        "an already SACKed entry has no RTT sample"
    );

    let second = model.send(2_000, 3_000, 3_000);
    assert_eq!(
        model.rack_update_rtt(second, None, 3_800, CUM_ACKED, 3_000),
        Some(800)
    );
    assert_eq!(
        model.rack_min_rtt, 500,
        "a larger sample cannot raise the minimum"
    );

    let retransmitted = model.send(3_000, 4_000, 4_000);
    model.rack_update_rsm(retransmitted, 4_500, RACK_SENT_FP, 1_000);
    assert_eq!(
        model.rack_update_rtt(retransmitted, Some(400), 4_600, SACKED, 0),
        None,
        "a retransmit acked below the minimum is spurious"
    );

    let no_rtt = model.send(4_000, 5_000, 5_000);
    if let Some(entry) = model.map.find_id_mut(no_rtt) {
        entry.no_rtt_allowed = true;
    }
    assert_eq!(
        model.rack_update_rtt(no_rtt, Some(700), 5_700, SACKED, 0),
        None
    );
}

#[test]
fn rack_update_seg_keeps_the_pacing_minimum_at_the_mss() {
    let mut model = base_model();
    model.pace_min_segs = 0;
    model.pace_max_segs = 0;
    model.user_set_max_segs = 2;
    assert!(model.rack_update_seg());
    assert_eq!(model.pace_min_segs, 1_000);
    assert_eq!(model.pace_max_segs, 0);
    assert!(!model.rack_update_seg(), "second run is unchanged");

    model.force_max_seg = true;
    assert!(model.rack_update_seg());
    assert_eq!(model.pace_max_segs, 2_000);
    assert!(!model.rack_update_seg());
}

#[test]
fn rack_what_can_we_send_is_limited_by_cwnd_rwnd_and_avail() {
    let mut model = base_model();
    model.snd_wnd = 3_000;
    assert_eq!(model.rack_what_can_we_send(5_000, 10_000, 0), 3_000);
    model.send(1_000, 2_000, 1_000);
    assert_eq!(model.rack_what_can_we_send(5_000, 10_000, 0), 2_000);
    assert_eq!(model.rack_what_can_we_send(5_000, 1_500, 500), 1_000);
    model.send(2_000, 4_000, 1_100);
    assert_eq!(
        model.rack_what_can_we_send(5_000, 10_000, 0),
        0,
        "outstanding data reached the peer window"
    );
    model.snd_wnd = 10_000;
    assert_eq!(
        model.rack_what_can_we_send(2_000, 10_000, 0),
        0,
        "flight size reached the cwnd"
    );

    let mut windowed = base_model();
    windowed.snd_wnd = 5_000;
    windowed.send(1_000, 4_000, 1_000);
    windowed.sacked = 2_000;
    assert_eq!(
        windowed.rack_what_can_we_send(10_000, 10_000, 0),
        2_000,
        "the rwnd cap trims the cwnd allowance by the outstanding bytes"
    );
}

#[test]
fn rack_init_retransmit_value_selects_the_three_modes() {
    let mut model = base_model();
    model.rack_init_retransmit_value(1);
    assert!(model.full_size_rxt);
    assert!(!model.shape_rxt_to_pacing_min);
    model.rack_init_retransmit_value(2);
    assert!(!model.full_size_rxt);
    assert!(model.shape_rxt_to_pacing_min);
    model.rack_init_retransmit_value(0);
    assert!(!model.full_size_rxt);
    assert!(!model.shape_rxt_to_pacing_min);
    model.rack_init_retransmit_value(7);
    assert!(!model.full_size_rxt);
    assert!(!model.shape_rxt_to_pacing_min);
}

#[test]
fn rack_strike_dupack_retransmits_on_the_third_dupack() {
    let mut model = base_model();
    let ids = fill(&mut model);
    assert_eq!(model.rack_strike_dupack(1_000, 0, 5_000), None);
    assert_eq!(model.rack_strike_dupack(1_000, 0, 5_000), None);
    assert_eq!(model.rack_strike_dupack(1_000, 0, 5_000), Some(ids[0]));
    assert_eq!(model.map.find_id(ids[0]).expect("entry").dupack, 3);
    assert!(model.in_fast_recovery);
    assert!(model.want_output);
    assert!(model.timer_override);

    let mut sack = base_model();
    sack.sack_permit = true;
    let ids = fill(&mut sack);
    sack.rack_min_rtt = 100;
    if let Some(entry) = sack.map.find_id_mut(ids[0]) {
        entry.flags |= RACK_SACK_PASSED;
    }
    assert_eq!(sack.rack_strike_dupack(1_000, 300, 1_100), None);
    assert_eq!(sack.rack_strike_dupack(1_000, 300, 1_100), None);
    assert_eq!(
        sack.rack_strike_dupack(1_000, 300, 1_100),
        None,
        "the RACK timer has not expired"
    );
    let mut sack = base_model();
    sack.sack_permit = true;
    let ids = fill(&mut sack);
    sack.rack_min_rtt = 100;
    if let Some(entry) = sack.map.find_id_mut(ids[0]) {
        entry.flags |= RACK_SACK_PASSED;
    }
    assert_eq!(sack.rack_strike_dupack(1_000, 300, 1_200), None);
    assert_eq!(sack.rack_strike_dupack(1_000, 300, 1_200), None);
    assert_eq!(sack.rack_strike_dupack(1_000, 300, 2_000), Some(ids[0]));
}

#[test]
fn rack_ack_received_applies_abc_and_the_early_recovery_clamp() {
    let mut model = base_model();
    model.send(1_000, 2_000, 1_000);
    model.stack.tp.snd_cwnd = 10_000;
    model.stack.tp.snd_ssthresh = 5_000;
    model.snd_wnd = 65_535;
    let outcome = model.rack_ack_received(2_000, 1, false);
    assert_eq!(outcome.acked, 1_000);
    assert_eq!(outcome.bytes_this_ack, 1_000);
    assert!(outcome.cwnd_limited);
    assert!(!outcome.abc_sentawnd);
    assert_eq!(model.bytes_acked, 1_000);

    model.bytes_acked = 9_500;
    let outcome = model.rack_ack_received(2_000, 1, false);
    assert!(outcome.abc_sentawnd);
    assert_eq!(model.bytes_acked, 500);

    model.stack.tp.snd_cwnd = 30_000;
    model.snd_wnd = 20_000;
    assert!(!model.rack_ack_received(2_000, 1, false).cwnd_limited);

    model.early_recovery_segs = 2;
    let clamped = model.rack_ack_received(5_000, 1, true);
    assert_eq!(clamped.acked, 4_000);
    assert_eq!(
        clamped.bytes_this_ack, 2_000,
        "early recovery clamps to rc_early_recovery_segs * MSS"
    );
}

#[test]
fn rack_process_ack_advances_una_and_exits_recovery() {
    let mut model = base_model();
    fill(&mut model);
    assert_eq!(
        model.rack_process_ack(500, 65_535, 0, &[], 0, 2_000),
        ProcessAck::Old
    );
    assert_eq!(
        model.rack_process_ack(9_999, 65_535, 0, &[], 0, 2_000),
        ProcessAck::AboveMax
    );
    assert!(model.want_output);

    let result = model.rack_process_ack(2_000, 65_535, 0, &[], 0, 2_000);
    let ProcessAck::Acked(outcome) = result else {
        panic!("expected an advancing ACK, got {result:?}");
    };
    assert_eq!(outcome.acked, 1_000);
    assert!(!outcome.post_recovery);
    assert_eq!(model.stack.tp.snd_una, 2_000);
    assert_eq!(model.map.segments.first().expect("head").start, 2_000);

    let duplicate = model.rack_process_ack(2_000, 65_535, 0, &[], 0, 2_100);
    assert!(matches!(duplicate, ProcessAck::Duplicate { .. }));

    model.in_fast_recovery = true;
    model.snd_recover = 4_000;
    model.stack.tp.snd_cwnd = 10_000;
    model.stack.tp.snd_ssthresh = 5_000;
    let result = model.rack_process_ack(4_000, 65_535, 0, &[], 0, 3_000);
    let ProcessAck::Acked(outcome) = result else {
        panic!("expected recovery exit, got {result:?}");
    };
    assert!(outcome.post_recovery);
    assert!(!model.in_fast_recovery);
    assert_eq!(model.stack.tp.snd_una, 4_000);
    assert!(model.map.segments.is_empty());
    assert_eq!(model.snd_recover, 4_000);
}

#[test]
fn rack_fastack_accepts_only_clean_advancing_acks() {
    let mut model = base_model();
    fill(&mut model);
    let base = FastAck {
        ack: 500,
        window: 4_000,
        tlen: 0,
        sacks: &[],
        sendbuf_avail: 100,
        now_us: 2_000,
        timestamp_ok: true,
    };
    assert!(!model.rack_fastack(base));
    assert!(!model.rack_fastack(FastAck { ack: 9_999, ..base }));
    assert!(!model.rack_fastack(FastAck { window: 0, ..base }));
    assert!(!model.rack_fastack(FastAck {
        timestamp_ok: false,
        ..base
    }));
    model.in_fast_recovery = true;
    assert!(!model.rack_fastack(FastAck { ack: 2_000, ..base }));
    model.in_fast_recovery = false;
    model.sacked = 1;
    assert!(!model.rack_fastack(FastAck { ack: 2_000, ..base }));

    let mut clean = base_model();
    fill(&mut clean);
    assert!(clean.rack_fastack(FastAck {
        ack: 2_000,
        window: 4_000,
        tlen: 0,
        sacks: &[],
        sendbuf_avail: 100,
        now_us: 2_000,
        timestamp_ok: true,
    }));
    assert_eq!(clean.stack.tp.snd_una, 2_000);
    assert_eq!(clean.snd_wnd, 4_000);
    assert_eq!(clean.map.segments.first().expect("head").start, 2_000);
    assert_eq!(clean.dupacks, 0);
    assert!(clean.want_output);
    assert_eq!(clean.hpts.rack_timer_cancel(clean.id), 0);

    let mut persist = base_model();
    fill(&mut persist);
    persist.in_persist = true;
    persist.high_rwnd = 1_000;
    persist.pace_min_segs = 1_000;
    assert!(persist.rack_fastack(FastAck {
        ack: 2_000,
        window: 600,
        tlen: 0,
        sacks: &[],
        sendbuf_avail: 0,
        now_us: 2_000,
        timestamp_ok: true,
    }));
    assert!(!persist.in_persist);
}

#[test]
fn rack_fast_rsm_output_builds_one_mss_and_a_tlp_probe() {
    let mut model = base_model();
    let ids = fill(&mut model);
    model.pace_max_segs = 1_000;
    model.prr_sndcnt = 3_000;
    model.in_fast_recovery = true;
    let tx = model
        .rack_fast_rsm_output(ids[0], 2_000, false, 5_000, 40)
        .expect("fast retransmit");
    assert_eq!(tx.seq, 1_000);
    assert_eq!(tx.len, 1_000);
    assert_eq!(tx.flags & TH_FIN, 0);
    assert_eq!(tx.flags & TH_ACK, TH_ACK);
    assert!(!tx.tso);
    assert!(!tx.tlp);
    assert_eq!(tx.map_updates, 1);
    let entry = model.map.find_id(ids[0]).expect("entry");
    assert_eq!(entry.rtr_cnt, 2);
    assert_eq!(entry.tim_lastsent[1], 5_000);
    assert_eq!(model.prr_sndcnt, 2_000);
    assert_eq!(model.retran_during_recovery, 1_000);
    assert_eq!(model.sndrexmitpack, 1);
    assert_eq!(model.sndrexmitbyte, 1_000);
    assert_eq!(model.fto_rsm_send, 1);
    assert_ne!(model.t_rtttime, 0);
    assert_eq!(model.t_rtseq, 1_000);

    let tx = model
        .rack_fast_rsm_output(ids[1], 1_000, true, 6_000, 40)
        .expect("TLP probe");
    assert!(tx.tlp);
    assert_eq!(tx.len, 1_000);
    assert_ne!(
        model.map.find_id(ids[1]).expect("entry").flags & RACK_TLP,
        0
    );
    assert_eq!(model.tlp_retran, 1);
    assert_eq!(model.tlp_retran_bytes, 1_000);
    assert!(model.tlp_in_progress);
    assert_eq!(model.tlp_cnt_out, 1);

    assert_eq!(
        model.rack_fast_rsm_output(9_999, 1_000, false, 7_000, 40),
        Err(RetransmitError::NoSegment)
    );
    if let Some(entry) = model.map.find_id_mut(ids[2]) {
        entry.flags |= RACK_HAS_FIN;
    }
    assert_eq!(
        model.rack_fast_rsm_output(ids[2], 1_000, false, 7_000, 40),
        Err(RetransmitError::Unsendable)
    );
    assert_eq!(
        model.rack_fast_rsm_output(ids[0], 100, false, 7_000, 40),
        Err(RetransmitError::TooShort)
    );
}

#[test]
fn rack_fast_rsm_output_uses_tso_across_adjacent_segments() {
    let mut model = base_model();
    let ids = fill(&mut model);
    model.pace_max_segs = 4_000;
    let tx = model
        .rack_fast_rsm_output(ids[0], 3_000, false, 5_000, 40)
        .expect("TSO retransmit");
    assert!(tx.tso);
    assert_eq!(tx.len, 3_000);
    assert_eq!(tx.segsiz, 1_000);
    assert_eq!(tx.map_updates, 3);
    for id in ids {
        assert_eq!(
            model.map.find_id(id).expect("entry").rtr_cnt,
            2,
            "every covered send-map entry is updated"
        );
    }
    assert_eq!(model.sndrexmitbyte, 3_000);
}
