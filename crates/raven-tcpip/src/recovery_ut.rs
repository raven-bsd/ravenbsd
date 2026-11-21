// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD RACK core tests.

use super::*;
use alloc::vec;

fn context(now_ms: u32, srtt_ms: u32) -> RackContext {
    RackContext {
        now_ms,
        srtt_ms,
        mss: 1_000,
        snd_max: 4_000,
        sack_permitted: true,
        dup_thresh: u32::from(RACK_DUP_THRESHOLD),
        std_based: true,
        use_dsack: true,
        ..RackContext::default()
    }
}

#[test]
fn rack_sack_block_marks_data_and_samples_rtt() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.send(2_000, 3_000, 0);
    let mut ctx = context(50, 20);
    ctx.ack = 1_000;
    ctx.snd_max = 3_000;
    ctx.sack_count = 1;
    ctx.sacks[0] = (2_000, 3_000);
    let outcome = rack.ack_received(&ctx);
    assert_eq!(outcome.acked, 1_000);
    assert_eq!(outcome.sacked, 1_000);
    assert_eq!(rack.sacked_bytes, 1_000);
    assert!(rack.entries.last().is_some_and(|entry| entry.sacked));
    assert_eq!(outcome.rtt_sample_us, Some(50_000));
    assert_eq!(rack.min_rtt_us, 50_000);
}

#[test]
fn rack_mark_sacked_splits_and_accounts() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 3_000, 0);
    rack.mark_sacked(1_200, 2_800);
    assert_eq!(rack.entries.len(), 3);
    assert_eq!(rack.entries[1].start, 1_200);
    assert_eq!(rack.entries[1].end, 2_800);
    assert!(rack.entries[1].sacked);
    assert_eq!(rack.sacked_bytes, 1_600);
}

#[test]
fn rack_peer_reneging_clears_a_sacked_range() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.mark_sacked(1_000, 2_000);
    let mut ctx = context(0, 20);
    ctx.ack = 1_000;
    ctx.snd_max = 2_000;
    let outcome = rack.ack_received(&ctx);
    assert_eq!(outcome.reneged, 1_000);
    assert_eq!(rack.sacked_bytes, 0);
    assert!(!rack.entries[0].sacked);
}

#[test]
fn rack_sack_passed_declares_loss_after_the_threshold() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.send(2_000, 3_000, 0);
    rack.send(3_000, 4_000, 0);
    let mut ctx = context(100, 40);
    ctx.ack = 1_000;
    ctx.snd_max = 4_000;
    ctx.sack_count = 1;
    ctx.sacks[0] = (3_000, 4_000);
    let outcome = rack.ack_received(&ctx);
    assert_eq!(outcome.lost, vec![1_000, 2_000]);
    assert!(outcome.entered_recovery);
    assert_eq!(rack.recover, 4_000);
    assert_eq!(rack.recover_fs, 3_000);
    assert_eq!(outcome.retransmit_range, Some((1_000, 2_000)));
    assert!(rack.considered_lost >= 2_000);
}

#[test]
fn rack_sack_passed_does_not_declare_loss_before_the_threshold() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.send(2_000, 3_000, 0);
    rack.send(3_000, 4_000, 0);
    let mut ctx = context(20, 40);
    ctx.ack = 1_000;
    ctx.snd_max = 4_000;
    ctx.sack_count = 1;
    ctx.sacks[0] = (3_000, 4_000);
    let outcome = rack.ack_received(&ctx);
    assert!(outcome.lost.is_empty());
    assert!(!rack.in_recovery);
    assert!(!rack.entries[0].lost);
}

#[test]
fn rack_dsack_counts_and_grows_the_reordering_window() {
    let mut rack = Rack::new(10);
    let mut ctx = context(100, 40);
    ctx.ack = 2_000;
    ctx.snd_max = 4_000;
    ctx.dsack_count = 1;
    ctx.dsacks[0] = (1_000, 2_000);
    let outcome = rack.ack_received(&ctx);
    assert!(outcome.dsack);
    assert_eq!(rack.num_dsack, 1);
    assert!(rack.reorder_window > 10);
    assert!(rack.dsack_grown);
}

#[test]
fn rack_dsack_equal_to_retransmitted_bytes_marks_a_spurious_recovery() {
    let mut rack = Rack::new(10);
    rack.retran_during_recovery = 500;
    let mut ctx = context(100, 40);
    ctx.ack = 2_000;
    ctx.snd_max = 4_000;
    ctx.dsack_count = 1;
    ctx.dsacks[0] = (1_000, 1_500);
    let outcome = rack.ack_received(&ctx);
    assert!(outcome.spurious);
    assert!(rack.might_revert);
    assert_eq!(rack.dsack_byte_cnt, 0);
}

#[test]
fn rack_rtt_ignores_a_retransmit_faster_than_min_rtt() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    let mut ctx = context(40, 20);
    ctx.ack = 2_000;
    ctx.snd_max = 2_000;
    let first = rack.ack_received(&ctx);
    assert_eq!(first.rtt_sample_us, Some(40_000));
    assert_eq!(rack.min_rtt_us, 40_000);

    rack.send(2_000, 3_000, 0);
    rack.update_entry(2_000, 1_000, 100);
    let mut ctx = context(110, 20);
    ctx.ack = 3_000;
    ctx.snd_max = 3_000;
    let second = rack.ack_received(&ctx);
    assert_eq!(second.rtt_sample_us, None);
    assert_eq!(rack.min_rtt_us, 40_000);
}

#[test]
fn rack_timestamp_echo_on_an_older_transmit_is_spurious() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.update_entry(1_000, 1_000, 60);
    let mut ctx = context(70, 20);
    ctx.ack = 2_000;
    ctx.snd_max = 2_000;
    ctx.ts_echo = Some(0);
    let outcome = rack.ack_received(&ctx);
    assert_eq!(outcome.rtt_sample_us, None);
}

#[test]
fn rack_update_entry_splits_a_partial_retransmission() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 3_000, 0);
    rack.update_entry(1_000, 500, 10);
    assert_eq!(rack.entries.len(), 2);
    assert_eq!(rack.entries[0].start, 1_000);
    assert_eq!(rack.entries[0].end, 1_500);
    assert_eq!(rack.entries[0].rtr_count, 2);
    assert_eq!(rack.entries[0].xmit_ts, 10);
    assert_eq!(rack.entries[1].start, 1_500);
    assert_eq!(rack.entries[1].rtr_count, 1);
    assert_eq!(rack.holes_rxt, 500);
    assert_eq!(rack.retran_during_recovery, 500);
}

#[test]
fn rack_cong_signal_enters_recovery_and_sets_prr() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 5_000, 0);
    let mut ctx = context(10, 20);
    ctx.ack = 2_000;
    ctx.snd_una = 2_000;
    ctx.snd_max = 5_000;
    ctx.cwnd = 5_000;
    ctx.ssthresh = 3_000;
    let outcome = rack.rack_cong_signal(&ctx, RackSignal::DupAck);
    assert!(outcome.in_recovery);
    assert!(outcome.send_cwr);
    assert_eq!(outcome.recover, 5_000);
    assert_eq!(outcome.prr_sndcnt, 1_000);
    assert!(rack.in_recovery);
    assert_eq!(rack.recover_fs, 3_000);
}

#[test]
fn rack_recovery_exits_at_the_recovery_point() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 5_000, 0);
    let mut ctx = context(10, 20);
    ctx.ack = 2_000;
    ctx.snd_una = 2_000;
    ctx.snd_max = 5_000;
    ctx.cwnd = 5_000;
    ctx.ssthresh = 3_000;
    rack.cong_signal(&ctx, RackSignal::DupAck);
    let mut done = context(20, 20);
    done.ack = 5_000;
    done.snd_max = 5_000;
    let outcome = rack.ack_received(&done);
    assert!(outcome.recovered);
    assert!(!rack.in_recovery);
}

#[test]
fn rack_rto_exits_recovery_and_rto_error_restores_it() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 5_000, 0);
    let mut ctx = context(10, 20);
    ctx.ack = 2_000;
    ctx.snd_una = 2_000;
    ctx.snd_max = 5_000;
    ctx.cwnd = 5_000;
    ctx.ssthresh = 3_000;
    ctx.mss = 1_000;
    rack.cong_signal(&ctx, RackSignal::DupAck);
    let rto = rack.cong_signal(&ctx, RackSignal::Rto);
    assert!(!rto.in_recovery);
    assert_eq!(rto.cwnd, 1_000);
    let undo = rack.cong_signal(&ctx, RackSignal::RtoErr);
    assert!(undo.in_recovery);
    assert_eq!(undo.recover, 5_000);
}

#[test]
fn rack_next_retransmit_requires_sack_pass_and_time() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    let ctx = context(100, 40);
    assert_eq!(rack.next_retransmit(&ctx), None);
    rack.entries[0].sack_passed = true;
    assert_eq!(rack.next_retransmit(&ctx), Some((1_000, 2_000)));
    rack.entries[0].sacked = true;
    assert_eq!(rack.next_retransmit(&ctx), None);
}

#[test]
fn rack_tlp_retransmits_the_highest_hole_and_is_bounded() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.send(2_000, 3_000, 0);
    let mut ctx = context(0, 10);
    ctx.snd_max = 3_000;
    rack.tlp_arm(&ctx);
    assert_eq!(rack.tlp_deadline_us, Some(20_000));
    let mut first = context(20, 10);
    first.snd_max = 3_000;
    assert_eq!(rack.tlp_fire(&first), Some((2_000, 3_000)));
    assert!(rack.entries[1].tlp);
    assert_eq!(rack.entries[1].rtr_count, 2);
    let mut second = context(40, 10);
    second.snd_max = 3_000;
    assert!(rack.tlp_fire(&second).is_some());
    let mut third = context(60, 10);
    third.snd_max = 3_000;
    assert_eq!(rack.tlp_fire(&third), None);
}

#[test]
fn rack_pipe_and_prr_follow_the_recovery_state() {
    let mut rack = Rack::new(10);
    rack.snd_max = 5_000;
    rack.snd_una = 1_000;
    rack.sacked_bytes = 1_000;
    rack.considered_lost = 500;
    rack.holes_rxt = 100;
    assert_eq!(rack.compute_pipe(1_000), 2_600);

    let mut ctx = context(10, 20);
    ctx.ack = 1_000;
    ctx.snd_una = 1_000;
    ctx.snd_max = 5_000;
    ctx.ssthresh = 3_000;
    ctx.mss = 1_000;
    rack.enter_recovery(&ctx);
    let allowance = rack.update_prr(1_000, &ctx);
    assert!(allowance > 0);
}

#[test]
fn rack_reorder_threshold_uses_srtt_dsack_and_the_ceiling() {
    let mut rack = Rack::new(10);
    rack.reorder_ts_us = 1;
    let ctx = context(0, 40);
    assert_eq!(rack.reorder_threshold_us(&ctx, 1_000), 50_000);
    rack.num_dsack = 1;
    assert_eq!(rack.reorder_threshold_us(&ctx, 1_000), 60_000);
    rack.num_dsack = 10;
    assert_eq!(rack.reorder_threshold_us(&ctx, 1_000), 80_000);
}

#[test]
fn rack_highest_unacked_skips_sacked_ranges() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.send(2_000, 3_000, 0);
    rack.send(3_000, 4_000, 0);
    rack.mark_sacked(3_000, 4_000);
    assert_eq!(rack.highest_unacked(), Some((2_000, 3_000)));
}

#[test]
fn rack_process_sack_block_splits_across_multiple_ranges() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.send(2_000, 3_000, 0);
    rack.send(3_000, 4_000, 0);
    rack.mark_sacked(1_500, 3_500);
    assert_eq!(rack.sacked_bytes, 2_000);
    assert_eq!(rack.entries.len(), 5);
    assert_eq!(rack.entries[0].start, 1_000);
    assert_eq!(rack.entries[0].end, 1_500);
    assert!(rack.entries[1].sacked);
    assert!(rack.entries[2].sacked);
    assert!(rack.entries[3].sacked);
    assert_eq!(rack.entries[4].start, 3_500);
}

#[test]
fn rack_update_entry_spans_multiple_ranges() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 3_000, 0);
    rack.update_entry(1_500, 2_000, 10);
    assert_eq!(rack.entries.len(), 3);
    assert_eq!(rack.entries[0].start, 1_000);
    assert_eq!(rack.entries[0].rtr_count, 1);
    assert_eq!(rack.entries[1].start, 1_500);
    assert_eq!(rack.entries[1].rtr_count, 2);
    assert_eq!(rack.entries[2].start, 3_000);
    assert_eq!(rack.entries[2].end, 3_500);
    assert_eq!(rack.holes_rxt, 1_500);
}

#[test]
fn rack_reorder_exit_after_a_fast_retransmit_ack() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.send(2_000, 3_000, 0);
    let mut first = context(100, 40);
    first.ack = 1_000;
    first.snd_max = 3_000;
    first.sack_count = 1;
    first.sacks[0] = (2_000, 3_000);
    let entered = rack.ack_received(&first);
    assert!(entered.entered_recovery);
    rack.update_entry(1_000, 1_000, 100);
    assert_eq!(rack.recovery_rxt_cnt, 1_000);

    let mut second = context(120, 40);
    second.ack = 1_000;
    second.snd_max = 3_000;
    second.sack_count = 2;
    second.sacks[0] = (1_000, 2_000);
    second.sacks[1] = (2_000, 3_000);
    let exited = rack.ack_received(&second);
    assert!(exited.recovered);
    assert!(!rack.in_recovery);
    assert!(rack.reorder_ts_us > 0);
}

#[test]
fn rack_dsack_inside_a_tlp_is_not_counted_for_reordering() {
    let mut rack = Rack::new(10);
    rack.last_sent_tlp = Some((1_000, 2_000));
    let mut outcome = RackOutcome::default();
    let ctx = context(100, 40);
    rack.note_dsack(1_000, 1_500, &ctx, &mut outcome);
    assert_eq!(rack.num_dsack, 0);
    assert_eq!(rack.dsack_byte_cnt, 500);
    assert!(outcome.dsack);
}

#[test]
fn rack_reorder_threshold_non_standard_and_fade() {
    let mut rack = Rack::new(10);
    rack.reorder_ts_us = 1;
    rack.std_based = false;
    let ctx = context(0, 40);
    assert_eq!(rack.reorder_threshold_us(&ctx, 1_000), 51_000);
    rack.reorder_fade_us = 500;
    assert_eq!(rack.reorder_threshold_us(&ctx, 10_000), 41_000);
    assert_eq!(rack.reorder_ts_us, 0);
}

#[test]
fn rack_prr_low_pipe_allows_two_segments() {
    let mut rack = Rack::new(10);
    rack.in_recovery = true;
    rack.snd_max = 2_000;
    rack.snd_una = 1_000;
    rack.sacked_bytes = 500;
    let mut ctx = context(10, 20);
    ctx.ack = 1_000;
    ctx.ssthresh = 5_000;
    ctx.mss = 1_000;
    assert_eq!(rack.update_prr(0, &ctx), 2_000);
}

#[test]
fn rack_ecn_signal_sets_recover_beyond_snd_max() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 3_000, 0);
    let mut ctx = context(10, 20);
    ctx.ack = 1_000;
    ctx.snd_max = 3_000;
    let outcome = rack.cong_signal(&ctx, RackSignal::Ecn);
    assert_eq!(outcome.recover, 3_001);
    assert!(outcome.send_cwr);
    assert!(!rack.in_recovery);
}

#[test]
fn rack_reneged_range_can_be_declared_lost_later() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.mark_sacked(1_000, 2_000);
    let mut ctx = context(100, 40);
    ctx.ack = 1_000;
    ctx.snd_max = 2_000;
    let first = rack.ack_received(&ctx);
    assert_eq!(first.reneged, 1_000);
    rack.entries[0].dupack = RACK_DUP_THRESHOLD;
    let second = rack.ack_received(&ctx);
    assert_eq!(second.lost, vec![1_000]);
    assert!(second.entered_recovery);
}

#[test]
fn rack_strike_dupack_returns_a_range_at_the_threshold() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    assert_eq!(rack.strike_dupack(), None);
    assert_eq!(rack.strike_dupack(), None);
    assert_eq!(rack.strike_dupack(), Some((1_000, 2_000)));
    assert_eq!(rack.entries[0].dupack, RACK_DUP_THRESHOLD);
}

#[test]
fn rack_ack_received_duplicate_ack_strikes_the_oldest_hole() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    let mut ctx = context(10, 20);
    ctx.ack = 0;
    ctx.snd_max = 2_000;
    let outcome = rack.ack_received(&ctx);
    assert!(outcome.dup_ack);
    assert_eq!(rack.entries[0].dupack, 1);
}

#[test]
fn rack_ack_received_old_ack_marks_reordering_without_advancing() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.snd_una = 2_000;
    rack.snd_max = 2_000;
    let mut ctx = context(10, 20);
    ctx.ack = 1_000;
    ctx.snd_wnd = 10_000;
    ctx.snd_max = 2_000;
    let outcome = rack.ack_received(&ctx);
    assert_eq!(outcome.acked, 0);
    assert_eq!(rack.snd_una, 2_000);
    assert!(rack.reorder_ts_us > 0);
}

#[test]
fn rack_ack_received_reports_abc_sent_awnd() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.t_bytes_acked = 2_999;
    let mut ctx = context(10, 20);
    ctx.ack = 1_000;
    ctx.snd_max = 2_000;
    ctx.cwnd = 3_000;
    ctx.ssthresh = 2_000;
    ctx.mss = 1_460;
    let outcome = rack.ack_received(&ctx);
    assert!(outcome.abc_sent_awnd);
    assert_eq!(rack.t_bytes_acked, 999);
}

#[test]
fn rack_ack_received_caps_early_recovery_cc_bytes() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 3_000, 0);
    rack.in_recovery = true;
    let mut ctx = context(10, 20);
    ctx.ack = 2_000;
    ctx.snd_max = 3_000;
    ctx.early_recovery_segs = 1;
    ctx.mss = 1_000;
    let outcome = rack.ack_received(&ctx);
    assert_eq!(outcome.acked, 2_000);
    assert_eq!(outcome.cc_acked, 1_000);
}

#[test]
fn rack_post_recovery_restores_rto_ssthresh_and_ages_dsack() {
    let mut rack = Rack::new(10);
    rack.in_recovery = true;
    rack.rto_ssthresh = 8_000;
    rack.dsack_persist = 2;
    rack.num_dsack = 3;
    let mut ctx = context(10, 20);
    ctx.ssthresh = 5_000;
    ctx.mss = 1_000;
    let cwnd = rack.rack_post_recovery(&ctx, 100);
    assert_eq!(cwnd, 2_000);
    assert_eq!(rack.last_ssthresh, 8_000);
    assert_eq!(rack.dsack_persist, 1);
    assert!(!rack.in_recovery);
}

#[test]
fn rack_calc_thresh_tlp_uses_the_donor_threshold() {
    let mut rack = Rack::new(10);
    let ctx = context(0, 40);
    assert_eq!(rack.rack_calc_thresh_tlp(&ctx), 80_000);
    rack.tlp_threshold = 2;
    assert_eq!(rack.rack_calc_thresh_tlp(&ctx), 60_000);
    rack.tlp_threshold = 0;
    assert_eq!(rack.rack_calc_thresh_tlp(&ctx), 80_000);
}

#[test]
fn rack_timer_start_selects_rxt_rack_and_tlp() {
    let mut rack = Rack::new(10);
    let mut ctx = context(100, 40);
    ctx.sack_permitted = false;
    assert_eq!(rack.rack_timer_start(&ctx), None);
    rack.send(1_000, 2_000, 0);
    let rxt = rack.rack_timer_start(&ctx).expect("rxt");
    assert_eq!(rxt.kind, RackTimerKind::Rxt);
    assert_eq!(rxt.flags, PACE_TMR_RXT);
    ctx.sack_permitted = true;
    let rack_timer = rack.rack_timer_start(&ctx).expect("rack");
    assert_eq!(rack_timer.kind, RackTimerKind::Rack);
    rack.tlp_arm(&ctx);
    let tlp = rack.rack_timer_start(&ctx).expect("tlp");
    assert_eq!(tlp.kind, RackTimerKind::Tlp);
    assert_eq!(tlp.flags, PACE_TMR_TLP);
}

#[test]
fn rack_peer_reneges_clears_contiguous_sacked_ranges() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.mark_sacked(1_000, 1_500);
    rack.mark_sacked(1_500, 2_000);
    assert_eq!(rack.sacked_bytes, 1_000);
    assert_eq!(rack.rack_peer_reneges(1_000), 1_000);
    assert_eq!(rack.sacked_bytes, 0);
    assert!(rack.entries.iter().all(|entry| !entry.sacked));
}

#[test]
fn rack_strike_dupack_skips_ranges_already_at_the_threshold() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.send(2_000, 3_000, 0);
    rack.entries[0].dupack = RACK_DUP_THRESHOLD;
    assert_eq!(rack.rack_strike_dupack(), None);
    assert_eq!(rack.entries[1].dupack, 1);
    assert_eq!(rack.rack_strike_dupack(), None);
    assert_eq!(rack.rack_strike_dupack(), Some((2_000, 3_000)));
}

#[test]
fn rack_update_rsm_records_was_sackpass_and_clears_sacked() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.entries[0].sack_passed = true;
    rack.rack_update_rsm(1_000, 10);
    assert!(!rack.entries[0].sack_passed);
    assert!(rack.entries[0].was_sacked);
    assert_eq!(rack.recovery_rxt_cnt, 1_000);
}

#[test]
fn rack_log_sack_passed_marks_only_older_ranges() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.send(2_000, 3_000, 0);
    rack.send(3_000, 4_000, 0);
    let ctx = context(100, 40);
    let mut outcome = RackOutcome::default();
    rack.rack_log_sack_passed(&ctx, 2_500, &mut outcome);
    assert!(rack.entries[0].sack_passed);
    assert!(rack.entries[1].sack_passed);
    assert!(!rack.entries[2].sack_passed);
}

#[test]
fn rack_proc_sack_blk_partial_is_idempotent() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 3_000, 0);
    let ctx = context(50, 20);
    let first = rack.rack_proc_sack_blk(&ctx, 1_200, 1_800);
    assert_eq!(first.sacked, 600);
    let second = rack.rack_proc_sack_blk(&ctx, 1_200, 1_800);
    assert_eq!(second.sacked, 0);
    assert_eq!(rack.sacked_bytes, 600);
}

#[test]
fn rack_process_to_cumack_trims_a_partial_range() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    let ctx = context(50, 20);
    let outcome = rack.rack_process_to_cumack(&ctx, 1_500);
    assert_eq!(outcome.acked, 0);
    assert_eq!(rack.snd_una, 1_500);
    assert_eq!(rack.entries[0].start, 1_500);
}

#[test]
fn rack_tcp_rack_output_prefers_must_retransmit() {
    let mut rack = Rack::new(10);
    rack.send(1_000, 2_000, 0);
    rack.entries[0].must_rxt = true;
    let mut ctx = context(0, 20);
    ctx.sack_permitted = false;
    assert_eq!(rack.tcp_rack_output(&ctx), Some((1_000, 2_000)));
}

#[test]
fn rack_update_prr_high_pipe_uses_the_recovery_fraction() {
    let mut rack = Rack::new(10);
    rack.in_recovery = true;
    rack.snd_max = 10_000;
    rack.snd_una = 1_000;
    rack.recover_fs = 9_000;
    let mut ctx = context(10, 20);
    ctx.ack = 1_000;
    ctx.ssthresh = 4_000;
    ctx.mss = 1_000;
    assert_eq!(rack.rack_update_prr(900, &ctx), 401);
}
