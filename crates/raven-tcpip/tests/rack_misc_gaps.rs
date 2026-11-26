// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One named test for each K350 K334 missing symbol (K362).

use raven_tcpip::{Errno, RACK_RWND_COLLAPSED, RackMisc, RackSendmap, TCPS_CLOSED};

fn misc() -> RackMisc {
    RackMisc::new()
}

fn queue(rack: &mut RackMisc, bytes: &[u8]) {
    let mbuf = rack.os.m_getcl(0).expect("mbuf");
    assert!(rack.os.mbufs.write(mbuf, 0, bytes));
    assert!(rack.os.mbufs.set_len(mbuf, bytes.len()));
    // The SockBuf append helper lives on the socket buffer, not the aggregate context.
    let raven_tcpip::RackOs { snd, mbufs, .. } = &mut rack.os;
    snd.sbappendstream_locked(mbufs, Some(mbuf))
        .expect("append");
}

#[test]
fn rack_swap_beta_values_swaps_saved_values() {
    let mut rack = misc();
    rack.cc_is_newreno = true;
    assert!(rack.rack_swap_beta_values(700, 650));
    assert_eq!(rack.saved_beta, 700);
    assert_eq!(rack.saved_beta_ecn, 650);
    assert!(rack.pacing_cc_set);
    rack.cc_is_newreno = false;
    assert!(!rack.rack_swap_beta_values(1, 1));
}

#[test]
fn rc_init_window_uses_rfc6928() {
    let mut rack = misc();
    rack.tp.t_maxseg = 1_460;
    assert_eq!(rack.rc_init_window(), 14_600);
    rack.tp.t_maxseg = 500;
    assert_eq!(rack.rc_init_window(), 5_000);
}

#[test]
fn rack_validate_multipliers_at_or_above100_clamps_up() {
    let mut rack = misc();
    rack.per_gp_rec = 50;
    rack.per_gp_ca = 90;
    rack.per_gp_ss = 100;
    assert_eq!(rack.rack_validate_multipliers_at_or_above100(), 2);
    assert_eq!(
        (rack.per_gp_rec, rack.per_gp_ca, rack.per_gp_ss),
        (100, 100, 100)
    );
}

#[test]
fn rack_validate_multipliers_at_or_below_100_clamps_down() {
    let mut rack = misc();
    rack.per_gp_ca = 150;
    rack.per_gp_ss = 200;
    assert_eq!(rack.rack_validate_multipliers_at_or_below_100(), 2);
    assert_eq!((rack.per_gp_ca, rack.per_gp_ss), (100, 100));
}

#[test]
fn rack_update_multiplier_tracks_goodput_and_losses() {
    let mut rack = misc();
    rack.gp_dyn_mul = true;
    rack.always_pace = true;
    rack.last_gp_comp_bw = 1_000;
    assert_eq!(rack.rack_update_multiplier(0, 2_000, 0, 0), 1);
    assert_eq!(rack.rack_update_multiplier(1, 500, 0, 0), 2);
    rack.loss_count = 1;
    assert_eq!(rack.rack_update_multiplier(1, 500, 0, 0), 0);
}

#[test]
fn tcp_rack_partialack_wants_output_for_prr() {
    let mut rack = misc();
    rack.prr_sndcnt = 1;
    assert!(rack.tcp_rack_partialack());
    assert!(rack.wanted_output);
    rack.wanted_output = false;
    rack.prr_sndcnt = 0;
    rack.no_prr = false;
    assert!(!rack.tcp_rack_partialack());
}

#[test]
fn rack_cc_after_idle_raises_to_the_initial_window() {
    let mut rack = misc();
    rack.tp.t_maxseg = 1_460;
    rack.tp.snd_cwnd = 1;
    rack.rack_cc_after_idle();
    assert_eq!(rack.tp.snd_cwnd, 1_460);
    rack.tp.snd_cwnd = 2;
    rack.rack_cc_after_idle();
    assert_eq!(rack.tp.snd_cwnd, rack.rc_init_window());
}

#[test]
fn rack_adjust_orig_mlen_compensates_trailing_space() {
    let mut rack = misc();
    let mut map = RackSendmap::new(100, 200, 1);
    map.id = 1;
    map.soff = 20;
    map.orig_m_len = 100;
    map.orig_t_space = 10;
    rack.sendmaps.insert(1, map);
    assert!(rack.rack_adjust_orig_mlen(1));
    let map = rack.sendmaps.get(&1).expect("map");
    assert_eq!(map.soff, 10);
    assert_eq!(map.orig_m_len, 110);
    assert_eq!(map.orig_t_space, 0);
    assert!(!rack.rack_adjust_orig_mlen(1));
}

#[test]
fn rack_setup_offset_for_rsm_copies_the_offset() {
    let mut rack = misc();
    let mut source = RackSendmap::new(100, 200, 1);
    source.id = 1;
    source.soff = 5;
    source.orig_m_len = 50;
    source.orig_t_space = 2;
    rack.sendmaps.insert(1, source);
    let mut target = RackSendmap::new(150, 200, 1);
    target.id = 2;
    rack.sendmaps.insert(2, target);
    assert!(rack.rack_setup_offset_for_rsm(1, 2));
    let target = rack.sendmaps.get(&2).expect("target");
    assert_eq!(target.soff, 105);
    assert_eq!(target.orig_m_len, 50);
    assert_eq!(target.orig_t_space, 2);
}

#[test]
fn rack_send_ack_challange_forces_one_ack_then_flags_probe() {
    let mut rack = misc();
    rack.tp.t_flags = 0x0020;
    assert!(rack.rack_send_ack_challange());
    assert!(rack.forced_ack);
    assert_eq!(rack.tp.t_flags & 0x0020, 0);
    assert!(!rack.probe_not_answered);
    assert!(rack.rack_send_ack_challange());
    assert!(rack.probe_not_answered);
}

#[test]
fn rack_convert_rtts_clamps_to_the_donor_rto() {
    let mut rack = misc();
    rack.tp.t_srtt = 1_000;
    rack.tp.t_rttvar = 10;
    rack.rack_convert_rtts();
    assert_eq!(rack.tp.t_rxtcur, 30_000);
    rack.tp.t_srtt = 100_000;
    rack.tp.t_rttvar = 1_000;
    rack.rack_convert_rtts();
    assert_eq!(rack.tp.t_rxtcur, 104_000);
}

#[test]
fn rack_cc_conn_init_sets_ssthresh_and_caps_cwnd() {
    let mut rack = misc();
    rack.tp.t_maxseg = 1_460;
    rack.snd_wnd = 50_000;
    rack.tp.snd_ssthresh = 0;
    rack.tp.snd_cwnd = 100_000;
    rack.rack_cc_conn_init();
    assert_eq!(rack.tp.snd_ssthresh, 50_000);
    assert_eq!(rack.tp.snd_cwnd, rack.rc_init_window());
}

#[test]
fn tcp_rack_xmit_timer_tracks_low_high_and_best() {
    let mut rack = misc();
    assert_eq!(rack.tcp_rack_xmit_timer(100, 10, 1_000, 1, None, 1), 1_000);
    assert_eq!(rack.tcp_rack_xmit_timer(200, 10, 900, 1, None, 1), 900);
    assert_eq!(rack.rtt_lowest, 100);
    assert_eq!(rack.rtt_highest, 200);
    assert_eq!(rack.rtt_cnt, 2);
    assert_eq!(rack.best_us_rtt, 900);
    assert!(rack.rtt_valid);
}

#[test]
fn tcp_rack_xmit_timer_commit_updates_srtt_and_rxtcur() {
    let mut rack = misc();
    rack.tcp_rack_xmit_timer(100_000, 10, 100_000, 1, None, 1);
    rack.rate_sample_method = 0;
    assert_eq!(rack.tcp_rack_xmit_timer_commit(), 100_000);
    assert_eq!(rack.tp.t_srtt, 100_000);
    assert!(rack.tp.t_rttvar > 0);
    assert!(rack.tp.t_rxtcur >= 100_000);
    assert_eq!(rack.tcp_rack_xmit_timer_commit(), 100_000);
}

#[test]
fn rack_apply_updated_usrtt_keeps_a_lower_minimum() {
    let mut rack = misc();
    assert!(rack.rack_apply_updated_usrtt(5_000, 10));
    assert_eq!(rack.min_rtt_us, 5_000);
    assert_eq!(rack.lower_rtt_us_cts, 10);
    assert!(!rack.rack_apply_updated_usrtt(6_000, 20));
    assert!(rack.rack_apply_updated_usrtt(4_000, 30));
    assert_eq!(rack.min_rtt_us, 4_000);
}

#[test]
fn is_rsm_inside_declared_tlp_block_checks_the_range() {
    let mut rack = misc();
    rack.last_tlp_acked_start = 100;
    rack.last_tlp_acked_end = 200;
    assert!(rack.is_rsm_inside_declared_tlp_block(150, 180));
    assert!(!rack.is_rsm_inside_declared_tlp_block(50, 90));
    assert!(!rack.is_rsm_inside_declared_tlp_block(210, 220));
}

#[test]
fn rack_rsm_sender_update_advances_the_goodput_mark() {
    let mut rack = misc();
    let mut map = RackSendmap::new(100, 200, 5);
    map.id = 1;
    rack.sendmaps.insert(1, map);
    rack.gput_inprog = true;
    rack.gp_cumack_ts = 0;
    assert!(rack.rack_rsm_sender_update(1));
    assert_eq!(rack.gput_ack, 200);
    assert_eq!(rack.gp_cumack_ts, 5);
    assert!(!rack.rack_rsm_sender_update(1));
}

#[test]
fn rack_check_bottom_drag_raises_multipliers() {
    let mut rack = misc();
    rack.tp.t_maxseg = 1_460;
    rack.tp.snd_max = 100;
    rack.tp.snd_una = 100;
    rack.per_gp_rec = 50;
    assert!(rack.rack_check_bottom_drag(0, 0));
    assert_eq!(rack.per_gp_rec, 100);
    assert!(rack.dragged_bottom);
}

#[test]
fn rack_adjust_sendmap_head_aligns_the_head_entry() {
    let mut rack = misc();
    let mut map = RackSendmap::new(100, 200, 1);
    map.id = 1;
    rack.sendmaps.insert(1, map);
    rack.tp.snd_una = 150;
    assert_eq!(rack.rack_adjust_sendmap_head(), 1);
    let map = rack.sendmaps.get(&1).expect("map");
    assert_eq!(map.soff, 0);
    assert_eq!(map.orig_m_len, 50);
}

#[test]
fn rack_req_check_for_comp_frees_completed_requests() {
    let mut rack = misc();
    rack.reqs.push((0, 100));
    rack.reqs.push((100, 200));
    rack.reqs.push((200, 300));
    assert_eq!(rack.rack_req_check_for_comp(150), 1);
    assert_eq!(rack.reqs.len(), 2);
    assert_eq!(rack.rack_req_check_for_comp(250), 1);
}

#[test]
fn rack_collapsed_window_records_the_collapse_point() {
    let mut rack = misc();
    rack.snd_wnd = 1_000;
    rack.tp.snd_max = 2_000;
    rack.rack_collapsed_window(500, 500);
    assert!(rack.has_collapsed);
    assert_eq!(rack.last_collapse_point, 1_500);
    assert_eq!(rack.high_collapse_point, 2_000);
    assert!(rack.collapse_point_valid);
}

#[test]
fn rack_un_collapse_window_marks_entries_beyond_the_point() {
    let mut rack = misc();
    let mut first = RackSendmap::new(1400, 1600, 1);
    first.id = 1;
    rack.sendmaps.insert(1, first);
    let mut second = RackSendmap::new(100, 200, 1);
    second.id = 2;
    rack.sendmaps.insert(2, second);
    rack.has_collapsed = true;
    rack.last_collapse_point = 1_500;
    assert_eq!(rack.rack_un_collapse_window(), 1);
    assert!(!rack.has_collapsed);
    assert_ne!(
        rack.sendmaps.get(&1).expect("map").flags & RACK_RWND_COLLAPSED,
        0
    );
}

#[test]
fn rack_validate_fo_sendwin_up_stops_a_small_remainder() {
    let mut rack = misc();
    rack.tp.t_maxseg = 1_460;
    rack.fast_output = true;
    rack.snd_wnd = 1_050;
    rack.fsb_left_to_send = 100;
    assert!(!rack.rack_validate_fo_sendwin_up(1_000));
    assert_eq!(rack.fsb_left_to_send, 50);
}

#[test]
fn rack_check_data_after_close_closes_when_disallowed() {
    let mut rack = misc();
    assert!(rack.rack_check_data_after_close(1));
    assert_eq!(rack.tp.t_state, TCPS_CLOSED);
    let mut allowed = misc();
    allowed.allow_data_after_close = true;
    assert!(!allowed.rack_check_data_after_close(1));
}

#[test]
fn rack_new_round_starts_increments_the_rounds() {
    let mut rack = misc();
    assert_eq!(rack.rack_new_round_starts(), 1);
    assert_eq!(rack.roundends, 1);
    assert_eq!(rack.rack_new_round_starts(), 2);
}

#[test]
fn rack_new_round_setup_records_the_ack() {
    let mut rack = misc();
    rack.current_round = 7;
    assert_eq!(rack.rack_new_round_setup(123), 7);
    assert_eq!(rack.last_round_ack, 123);
}

#[test]
fn rack_fo_base_copym_allocates_a_chain() {
    let mut rack = misc();
    let id = rack.rack_fo_base_copym(16).expect("copy");
    assert_eq!(rack.os.mbufs.mbuf(id).expect("mbuf").len, 16);
}

#[test]
fn rack_fo_m_copym_copies_from_an_mbuf() {
    let mut rack = misc();
    let base = rack.rack_fo_base_copym(16).expect("base");
    assert!(rack.rack_fo_m_copym(base, 0, 8).is_some());
}

#[test]
fn rack_check_queue_level_tracks_the_high_water() {
    let mut rack = misc();
    rack.snd_cwnd = 10;
    assert!(!rack.rack_check_queue_level(5));
    assert!(rack.rack_check_queue_level(10));
    assert_eq!(rack.high_rwnd, 10);
}

#[test]
fn rack_fast_output_requires_data_and_fast_output() {
    let mut rack = misc();
    assert!(!rack.rack_fast_output());
    queue(&mut rack, &[0x5a; 100]);
    rack.snd_wnd = 1_000;
    rack.snd_cwnd = 500;
    rack.fast_output = true;
    assert!(rack.rack_fast_output());
    assert_eq!(rack.fsb_left_to_send, 500);
}

#[test]
fn rack_setup_fast_output_initializes_the_sendbuf_state() {
    let mut rack = misc();
    queue(&mut rack, &[0x5a; 100]);
    rack.snd_wnd = 1_000;
    rack.snd_cwnd = 500;
    assert!(rack.rack_setup_fast_output());
    assert!(rack.fast_output_started);
    assert!(rack.fast_output);
    assert!(rack.fsb.initialized);
    assert_eq!(rack.fsb_left_to_send, 500);
}

#[test]
fn rack_check_collapsed_reports_valid_collapse() {
    let mut rack = misc();
    assert!(!rack.rack_check_collapsed());
    rack.has_collapsed = true;
    rack.collapse_point_valid = true;
    assert!(rack.rack_check_collapsed());
}

#[test]
fn rack_validate_sizes_rejects_empty_and_oversize() {
    let rack = misc();
    assert_eq!(rack.rack_validate_sizes(0, 1_460), Err(Errno::EINVAL));
    assert_eq!(rack.rack_validate_sizes(1_000, 1_460), Ok(()));
    assert_eq!(rack.rack_validate_sizes(100_000, 1_460), Err(Errno::EINVAL));
}
