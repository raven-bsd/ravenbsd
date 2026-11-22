// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the K333 RACK pipe/lost/recovery functions.

use super::*;
use crate::rack_hpts::RackHpts;

fn base() -> RackPipe {
    let mut pipe = RackPipe::new();
    let tp = &mut pipe.stack.tp;
    tp.t_state = 4;
    tp.snd_una = 1_000;
    tp.snd_max = 5_000;
    tp.snd_cwnd = 10_000;
    tp.snd_ssthresh = 20_000;
    tp.t_maxseg = 1_000;
    tp.t_rxtcur = 200_000;
    pipe.stack.pace_min_segs = 1_000;
    pipe.rack_rtt = 1_000;
    pipe
}

fn entry(start: u32, end: u32, last: u64, flags: u32) -> RackSendmap {
    let mut rsm = RackSendmap::new(start, end, last);
    rsm.flags = flags;
    rsm
}

#[test]
fn do_rack_compute_pipe_subtracts_sacked_and_lost_and_adds_holes() {
    let mut pipe = base();
    pipe.stack.sacked = 100;
    pipe.stack.considered_lost = 200;
    pipe.stack.holes_rxt = 50;
    assert_eq!(pipe.do_rack_compute_pipe(1_000), 3_750);
    assert_eq!(pipe.rack_compute_pipe(), 3_750);
    assert_eq!(pipe.do_rack_compute_pipe(2_000), 2_750);

    pipe.stack.sacked = 0;
    pipe.stack.considered_lost = 0;
    pipe.stack.holes_rxt = 0;
    assert_eq!(pipe.do_rack_compute_pipe(1_000), 4_000);
    assert_eq!(pipe.rack_compute_pipe(), 4_000);
}

#[test]
fn rack_grab_rtt_prefers_the_rack_sample_then_srtt_then_initial() {
    let mut pipe = base();
    pipe.rack_rtt = 12_345;
    assert_eq!(pipe.rack_grab_rtt(), 12_345);
    pipe.rack_rtt = 0;
    pipe.stack.tp.t_srtt = 2_000;
    assert_eq!(pipe.rack_grab_rtt(), 2_000);
    pipe.stack.tp.t_srtt = 0;
    assert_eq!(pipe.rack_grab_rtt(), RACK_INITIAL_RTO);
}

#[test]
fn rack_mark_lost_marks_sack_passed_entries_after_the_threshold() {
    let mut pipe = base();
    pipe.push_sendmap(entry(1_000, 2_000, 1_000, 0));
    pipe.push_sendmap(entry(2_000, 3_000, 1_000, RACK_SACK_PASSED));
    pipe.push_sendmap(entry(3_000, 4_000, 1_000, RACK_SACK_PASSED));

    pipe.rack_mark_lost(1, 2_000);
    assert_ne!(pipe.sendmap()[1].flags & RACK_WAS_LOST, 0);
    assert_ne!(pipe.sendmap()[2].flags & RACK_WAS_LOST, 0);
    assert_eq!(pipe.stack.considered_lost, 2_000);

    let mut early = base();
    early.push_sendmap(entry(1_000, 2_000, 1_000, 0));
    early.push_sendmap(entry(2_000, 3_000, 1_000, RACK_SACK_PASSED));
    early.rack_mark_lost(1, 1_999);
    assert_eq!(early.sendmap()[1].flags & RACK_WAS_LOST, 0);
    assert_eq!(early.stack.considered_lost, 0);

    let mut blocked = base();
    blocked.push_sendmap(entry(1_000, 2_000, 1_000, 0));
    blocked.rack_mark_lost(0, 5_000);
    assert_eq!(blocked.stack.considered_lost, 0);
}

#[test]
fn rack_mark_nolonger_lost_clears_the_flag_and_decrements_the_count() {
    let mut pipe = base();
    pipe.stack.considered_lost = 2_500;
    pipe.push_sendmap(entry(2_000, 3_000, 1_000, RACK_WAS_LOST | RACK_SACK_PASSED));
    pipe.rack_mark_nolonger_lost(0);
    assert_eq!(pipe.sendmap()[0].flags & RACK_WAS_LOST, 0);
    assert_ne!(pipe.sendmap()[0].flags & RACK_SACK_PASSED, 0);
    assert_eq!(pipe.stack.considered_lost, 1_500);

    pipe.stack.considered_lost = 500;
    let mut rsm = pipe.sendmap()[0];
    rsm.flags |= RACK_WAS_LOST;
    pipe.sendmap_mut(0).expect("entry").clone_from(&rsm);
    pipe.rack_mark_nolonger_lost(0);
    assert_eq!(pipe.stack.considered_lost, 0);
}

#[test]
fn rack_calc_thresh_rack_follows_standard_reorder_and_dsack_rules() {
    let mut pipe = base();
    pipe.state.pkt_delay = 200;
    assert_eq!(pipe.rack_calc_thresh_rack(1_000, 1_000), 1_200);

    pipe.state.tmr_std_based = true;
    assert_eq!(pipe.rack_calc_thresh_rack(1_000, 1_000), 1_250);
    pipe.state.tmr_std_based = false;

    pipe.state.reorder_ts = 1_000;
    pipe.state.reorder_fade = 10_000;
    pipe.state.reorder_shift = 2;
    assert_eq!(pipe.rack_calc_thresh_rack(1_000, 1_000), 1_450);

    pipe.state.use_dsack = true;
    pipe.stack.num_dsacks = 2;
    assert_eq!(pipe.rack_calc_thresh_rack(1_000, 1_000), 1_950);

    pipe.stack.num_dsacks = 0;
    pipe.state.use_dsack = false;
    pipe.state.reorder_ts = 0;
    pipe.state.reorder_shift = 0;
    pipe.state.pkt_delay = 5_000;
    assert_eq!(pipe.rack_calc_thresh_rack(1_000, 1_000), 2_000);

    pipe.state.pkt_delay = 200;
    pipe.state.reorder_ts = 1_000;
    pipe.state.reorder_fade = 500;
    assert_eq!(pipe.rack_calc_thresh_rack(1_000, 2_000), 1_200);
    assert_eq!(pipe.state.reorder_ts, 0, "a faded reorder timestamp clears");
}

#[test]
fn rack_calc_thresh_tlp_uses_the_donor_threshold_and_caps() {
    let mut pipe = base();
    pipe.state.tlp_threshold = 2;
    pipe.state.tlp_threshold_use = TLP_USE_TWO_TWO;
    pipe.push_sendmap(entry(1_000, 2_000, 5_000, 0));
    assert_eq!(pipe.rack_calc_thresh_tlp(0, 50_000), 115_000);

    pipe.stack.tp.t_rxtcur = 80_000;
    assert_eq!(pipe.rack_calc_thresh_tlp(0, 50_000), 80_000);

    pipe.stack.tp.t_rxtcur = 200_000;
    pipe.state.tlp_threshold = 0;
    assert_eq!(pipe.rack_calc_thresh_tlp(0, 50_000), 115_000);

    pipe.state.tlp_threshold = 2;
    pipe.state.tlp_threshold_use = TLP_USE_TWO_ONE;
    pipe.push_sendmap(entry(2_000, 3_000, 10_000, 0));
    assert_eq!(pipe.rack_calc_thresh_tlp(1, 50_000), 80_000);

    pipe.state.tlp_threshold_use = TLP_USE_ID;
    pipe.state.tlp_min = 100_000;
    assert_eq!(pipe.rack_calc_thresh_tlp(0, 50_000), 100_000);

    pipe.state.tlp_min = 10_000;
    pipe.stack.tp.snd_max = 1_500;
    assert_eq!(pipe.rack_calc_thresh_tlp(0, 50_000), 115_000);
}

#[test]
fn rack_check_recovery_mode_finds_an_overdue_unacked_segment() {
    let mut pipe = base();
    pipe.push_sendmap(entry(1_000, 2_000, 1_000, RACK_ACKED));
    pipe.push_sendmap(entry(2_000, 3_000, 1_000, 0));
    assert_eq!(pipe.rack_check_recovery_mode(2_000), Some(1));
    assert_ne!(pipe.stack.tp.t_flags & TF_FASTRECOVERY, 0);
    assert_ne!(pipe.stack.tp.t_flags & TF_CONGRECOVERY, 0);
    assert_eq!(pipe.stack.tp.snd_ssthresh, 5_000);

    let mut early = base();
    early.push_sendmap(entry(1_000, 2_000, 1_000, 0));
    assert_eq!(early.rack_check_recovery_mode(1_999), None);
    assert_eq!(early.stack.tp.t_flags, 0);

    let mut acked = base();
    acked.push_sendmap(entry(1_000, 2_000, 1_000, RACK_ACKED));
    assert_eq!(acked.rack_check_recovery_mode(5_000), None);
    let mut empty = base();
    assert_eq!(empty.rack_check_recovery_mode(5_000), None);
}

#[test]
fn rack_enter_recovery_sets_prr_and_snd_recover() {
    let mut pipe = base();
    pipe.push_sendmap(entry(1_000, 2_000, 1_000, RACK_WAS_SACKPASS));
    pipe.sendmap_mut(0).expect("entry").rtr_cnt = 2;
    pipe.push_sendmap(entry(2_000, 2_500, 1_000, RACK_WAS_SACKPASS));
    pipe.sendmap_mut(1).expect("entry").rtr_cnt = 3;
    pipe.stack.prr_delivered = 7;
    pipe.stack.prr_out = 8;
    pipe.stack.prr_sndcnt = 9;
    pipe.stack.tp.t_flags = TF_WASFRECOVERY | TF_WASCRECOVERY;

    pipe.rack_enter_recovery();
    assert_eq!(
        pipe.stack.tp.t_flags & (TF_WASFRECOVERY | TF_WASCRECOVERY),
        0
    );
    assert_ne!(pipe.stack.tp.t_flags & TF_FASTRECOVERY, 0);
    assert_ne!(pipe.stack.tp.t_flags & TF_CONGRECOVERY, 0);
    assert_eq!(pipe.stack.tp.snd_ssthresh, 5_000);
    assert_eq!(pipe.stack.prr_delivered, 0);
    assert_eq!(pipe.stack.prr_out, 0);
    assert_eq!(pipe.stack.prr_sndcnt, 1_000);
    assert_eq!(pipe.stack.prr_recovery_fs, 4_000);
    assert_eq!(pipe.state.snd_recover, 5_000);
    assert_eq!(pipe.state.recovery_rxt_cnt, 2_000);
    assert_eq!(pipe.state.cwnd_at_erec, 10_000);
    assert_eq!(pipe.state.ssthresh_at_erec, 20_000);
    assert!(pipe.state.ent_rec_ns);
    assert!(pipe.state.initial_ss_comp);
    assert!(!pipe.state.fast_output);

    pipe.stack.tp.snd_ssthresh = 5_555;
    pipe.stack.prr_sndcnt = 42;
    pipe.rack_enter_recovery();
    assert_eq!(pipe.stack.tp.snd_ssthresh, 5_555);
    assert_eq!(pipe.stack.prr_sndcnt, 42);

    let mut ecn = base();
    ecn.stack.t_flags2 = TF2_ECN_PERMIT;
    ecn.rack_enter_recovery();
    assert_ne!(ecn.stack.t_flags2 & TF2_ECN_SND_CWR, 0);
}

#[test]
fn rack_exit_recovery_clears_both_recovery_flags() {
    let mut pipe = base();
    pipe.stack.tp.t_flags = TF_FASTRECOVERY | TF_CONGRECOVERY;
    pipe.rack_exit_recovery(3);
    assert_eq!(pipe.stack.tp.t_flags & IN_RECOVERY_MASK, 0);

    let mut clean = base();
    clean.rack_exit_recovery(1);
    assert_eq!(clean.stack.tp.t_flags, 0);
}

#[test]
fn rack_post_recovery_adds_back_prr_and_exits() {
    let mut pipe = base();
    pipe.stack.tp.snd_max = 1_000;
    pipe.stack.tp.snd_cwnd = 5_000;
    pipe.stack.tp.snd_ssthresh = 8_000;
    pipe.stack.tp.t_flags = TF_FASTRECOVERY | TF_CONGRECOVERY;
    pipe.stack.prr_sndcnt = 1_500;
    pipe.rack_post_recovery(1_000);
    assert_eq!(pipe.stack.tp.snd_cwnd, 9_500);
    assert_eq!(pipe.stack.tp.snd_ssthresh, 8_000);
    assert_eq!(pipe.stack.tp.t_flags & IN_RECOVERY_MASK, 0);
    assert_eq!(pipe.stack.prr_sndcnt, 0);
    assert_eq!(pipe.state.snd_recover, 1_000);

    let mut limited = base();
    limited.stack.tp.snd_max = 3_000;
    limited.stack.tp.snd_cwnd = 5_000;
    limited.stack.tp.snd_ssthresh = 8_000;
    limited.stack.tp.t_flags = TF_FASTRECOVERY | TF_CONGRECOVERY;
    limited.stack.prr_sndcnt = 1_500;
    limited.rack_post_recovery(3_000);
    assert_eq!(limited.stack.tp.snd_cwnd, 8_000);
    assert_eq!(limited.stack.prr_sndcnt, 0);

    let mut no_prr = base();
    no_prr.stack.tp.snd_cwnd = 5_000;
    no_prr.stack.tp.snd_ssthresh = 8_000;
    no_prr.stack.tp.t_flags = TF_FASTRECOVERY;
    no_prr.stack.prr_sndcnt = 1_500;
    no_prr.state.rack_no_prr = true;
    no_prr.rack_post_recovery(1_000);
    assert_eq!(no_prr.stack.tp.snd_cwnd, 8_000);
    assert_eq!(no_prr.stack.prr_sndcnt, 1_500);

    let mut dsack = base();
    dsack.stack.tp.snd_max = 1_000;
    dsack.stack.tp.t_flags = TF_FASTRECOVERY;
    dsack.stack.num_dsacks = 3;
    dsack.state.dsack_persist = 2;
    dsack.rack_post_recovery(1_000);
    assert_eq!(dsack.state.dsack_persist, 1);
    assert_eq!(dsack.stack.num_dsacks, 3);
    dsack.stack.tp.t_flags = TF_FASTRECOVERY;
    dsack.stack.prr_sndcnt = 0;
    dsack.state.dsack_persist = 1;
    dsack.rack_post_recovery(1_000);
    assert_eq!(dsack.state.dsack_persist, 0);
    assert_eq!(dsack.stack.num_dsacks, 0);

    let mut rto = base();
    rto.stack.tp.snd_max = 1_000;
    rto.stack.tp.snd_ssthresh = 8_000;
    rto.stack.tp.t_flags = TF_FASTRECOVERY;
    rto.state.rto_from_rec = true;
    rto.state.rto_ssthresh = 9_000;
    rto.rack_post_recovery(1_000);
    assert!(!rto.state.rto_from_rec);
    assert_eq!(rto.stack.tp.snd_ssthresh, 9_000);
}

#[test]
fn rack_pipe_uses_the_k313_hpts_clock() {
    let mut pipe = base();
    pipe.hpts = RackHpts::new(2_000);
    assert_eq!(pipe.now_us(), 2_000);
    pipe.push_sendmap(entry(1_000, 2_000, 1_000, 0));
    assert_eq!(pipe.rack_check_recovery_mode_now(), Some(0));

    let mut early = base();
    early.hpts = RackHpts::new(1_999);
    early.push_sendmap(entry(1_000, 2_000, 1_000, 0));
    assert_eq!(early.rack_check_recovery_mode_now(), None);
}
