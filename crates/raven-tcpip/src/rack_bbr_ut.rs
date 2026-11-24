// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the K328 RACK BBR/pacing donor functions.

use super::*;

fn stack() -> RackStack {
    let mut stack = RackStack::new();
    stack.tp.t_maxseg = 1000;
    stack.tp.t_srtt = 10_000;
    stack.tp.snd_una = 0;
    stack.tp.snd_max = 200_000;
    stack.tp.snd_nxt = 100_000;
    stack.tp.snd_cwnd = 100_000;
    stack.tp.snd_ssthresh = 200_000;
    stack.snd_wnd = 65_535;
    stack
}

fn filled() -> (RackStack, BbrRack) {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.gp_filled = true;
    bbr.gp_bw = 1_000_000;
    bbr.num_measurements = RACK_REQ_AVG;
    bbr.gp_srtt = 10_000;
    bbr.gp_min_rtt = 10_000;
    bbr.pace_min_segs = 1000;
    bbr.always_pace = true;
    bbr.gp_ready = true;
    bbr.per_gp_ca = 100;
    bbr.per_gp_ss = 50;
    stack.tp.snd_ssthresh = 1_000_000;
    (stack, bbr)
}

#[test]
fn rack_get_lt_bw_uses_bytes_and_time() {
    let stack = stack();
    let mut bbr = BbrRack::new();
    bbr.lt_bw_time = 2;
    bbr.lt_bw_bytes = 2_000_000;
    assert_eq!(bbr.get_lt_bw(&stack, 0), 1_000_000_000_000);
}

#[test]
fn rack_log_gpset_records_fields() {
    let stack = stack();
    let mut bbr = BbrRack::new();
    bbr.log_gpset(&stack, 5000, 7000, 1000, 77, 3, 10, 20);
    let event = *bbr.log.last().unwrap();
    assert_eq!(event.kind, 2);
    assert_eq!(event.line, 77);
    assert_eq!(event.fields[0], 5000);
    assert_eq!(event.fields[6], 3);
}

#[test]
fn rack_get_fixed_pacing_bw_selects_recovery_ss_ca() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    stack.options.pacing_rate_rec = 111;
    stack.options.pacing_rate_ss = 222;
    stack.options.pacing_rate_ca = 333;
    bbr.cwnd_to_use = 1000;
    stack.tp.snd_ssthresh = 2000;
    assert_eq!(bbr.get_fixed_pacing_bw(&stack), 222);
    bbr.cwnd_to_use = 3000;
    assert_eq!(bbr.get_fixed_pacing_bw(&stack), 333);
    stack.tp.t_flags |= TF_FASTRECOVERY;
    assert_eq!(bbr.get_fixed_pacing_bw(&stack), 111);
}

#[test]
fn rack_log_hybrid_bw_records_fields() {
    let stack = stack();
    let mut bbr = BbrRack::new();
    bbr.log_hybrid_bw(&stack, 9, 1_000, 2_000, 3_000, 8, 12, None);
    let event = *bbr.log.last().unwrap();
    assert_eq!(event.kind, 3);
    assert_eq!(event.fields[0], 9);
    assert_eq!(event.fields[4], 8);
}

#[test]
fn rack_compensate_for_linerate_adds_headers() {
    let stack = stack();
    let bbr = BbrRack::new();
    // MSS 1000 + TCP 20 + IPv4 20 + Ethernet 14 = 1054 bytes per 1000-byte segment.
    assert_eq!(bbr.compensate_for_linerate(&stack, 100_000), 105_400);
}

#[test]
fn rack_rate_cap_bw_clamps_to_the_cap() {
    let stack = stack();
    let mut bbr = BbrRack::new();
    bbr.bw_rate_cap = 5_000;
    let (bw, capped) = bbr.rate_cap_bw(&stack, 10_000, 0);
    assert_eq!(bw, 5_000);
    assert!(capped);
}

#[test]
fn rack_get_gp_est_initial_rate_and_lt_bw() {
    let stack = stack();
    let mut bbr = BbrRack::new();
    bbr.init_rate = 123_456;
    assert_eq!(bbr.get_gp_est(&stack, 0), 123_456);
    bbr.init_rate = 0;
    bbr.lt_bw_time = 1_000;
    bbr.lt_bw_bytes = 1_000_000;
    assert_eq!(bbr.get_gp_est(&stack, 0), 1_054_000_000);
}

#[test]
fn rack_get_bw_uses_fixed_rate_when_configured() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    stack.options.pacing_rate_ca = 555;
    bbr.cwnd_to_use = 1_000_000;
    bbr.use_fixed_rate = true;
    stack.tp.snd_ssthresh = 5_000;
    assert_eq!(bbr.get_bw(&stack, 0), 555);
}

#[test]
fn rack_get_output_bw_applies_gain_and_minimum() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.per_gp_ca = 100;
    bbr.per_gp_ss = 100;
    let (bw, capped) = bbr.get_output_bw(&stack, 100_000, false);
    assert_eq!(bw, 100_000);
    assert!(!capped);
    let (bw, _) = bbr.get_output_bw(&stack, 1, false);
    assert_eq!(bw, RACK_MIN_BW);
    bbr.hw_rate_cap = 50_000;
    assert_eq!(bbr.get_output_bw(&stack, 100_000, false).0, 50_000);
    stack.tp.t_flags |= TF_FASTRECOVERY;
}

#[test]
fn rack_get_output_gain_uses_recovery_and_configured_rate() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.per_gp_rec = 90;
    bbr.per_gp_ca = 100;
    bbr.per_gp_ss = 50;
    bbr.cwnd_to_use = 1_000;
    stack.tp.snd_ssthresh = 2_000;
    assert_eq!(bbr.get_output_gain(&stack, false), 50);
    bbr.cwnd_to_use = 3_000;
    assert_eq!(bbr.get_output_gain(&stack, false), 100);
    stack.tp.t_flags |= TF_FASTRECOVERY;
    assert_eq!(bbr.get_output_gain(&stack, true), 90);
}

#[test]
fn rack_log_hdwr_pacing_records_fields() {
    let stack = stack();
    let mut bbr = BbrRack::new();
    bbr.log_hdwr_pacing(&stack, 1_000, 2_000, -3, 4, 55);
    let event = *bbr.log.last().unwrap();
    assert_eq!(event.kind, 5);
    assert_eq!(event.fields[0], 1_000);
    assert_eq!(event.fields[2], (-3i64) as u64);
}

#[test]
fn rack_log_type_pacing_sizes_records_fields() {
    let stack = stack();
    let mut bbr = BbrRack::new();
    bbr.pace_min_segs = 1000;
    bbr.pace_max_segs = 4000;
    bbr.log_type_pacing_sizes(&stack, 900, 5000, 77, 2);
    let event = *bbr.log.last().unwrap();
    assert_eq!(event.kind, 7);
    assert_eq!(event.fields[0], 900);
    assert_eq!(event.fields[2], 1000);
}

#[test]
fn rack_get_measure_window_initial_and_filled() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    assert_eq!(bbr.get_measure_window(&stack, 0), 20_000);
    bbr.gp_filled = true;
    bbr.gp_bw = 1_000_000;
    bbr.pace_min_segs = 1000;
    bbr.tun.goal_bdp = 2;
    stack.tp.t_srtt = 10_000;
    assert!(bbr.get_measure_window(&stack, 0) >= 20_000);
}

#[test]
fn rack_enough_for_measurement_quality_paths() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.gput_seq = 0;
    bbr.gput_ack = 10_000;
    stack.tp.snd_max = 20_000;
    assert_eq!(
        bbr.enough_for_measurement(&stack, 20_000, 0),
        Some(Quality::AllAcked)
    );
    assert_eq!(
        bbr.enough_for_measurement(&stack, 15_000, 0),
        Some(Quality::High)
    );
    assert_eq!(bbr.enough_for_measurement(&stack, 1_000, 0), None);
}

#[test]
fn rack_log_timely_records_fields() {
    let mut bbr = BbrRack::new();
    bbr.log_timely(77, 9, [1, 2, 3, 4]);
    let event = *bbr.log.last().unwrap();
    assert_eq!(event.kind, 8);
    assert_eq!(event.line, 77);
    assert_eq!(event.fields[0], 9);
}

#[test]
fn rack_bw_can_be_raised_checks_pacing_overage() {
    let stack = stack();
    let mut bbr = BbrRack::new();
    assert!(bbr.bw_can_be_raised(&stack, 1_000_000, 1_000_000, 100));
    bbr.tun.max_per_above = 30;
    assert!(!bbr.bw_can_be_raised(&stack, 1_000_000, 1_000_000, 200));
}

#[test]
fn rack_increase_bw_mul_increments_selected_stage() {
    let stack = stack();
    let mut bbr = BbrRack::new();
    bbr.gp_saw_ss = true;
    bbr.per_gp_ss = 50;
    bbr.increase_bw_mul(&stack, 0, 1_000_000, 1_000_000, false);
    assert_eq!(bbr.per_gp_ss, 52);
}

#[test]
fn rack_decrease_bw_mul_clamps_to_lower_bound() {
    let mut bbr = BbrRack::new();
    bbr.gp_saw_ca = true;
    bbr.per_gp_ca = 100;
    bbr.gp_min_rtt = 10_000;
    bbr.decrease_bw_mul(1, 40_000, 10_000);
    assert_eq!(bbr.per_gp_ca, 50);
}

#[test]
fn rack_get_decrease_matches_the_integer_gradient() {
    let mut bbr = BbrRack::new();
    bbr.gp_min_rtt = 10_000;
    assert_eq!(bbr.get_decrease(100, 5_000), 60);
}

#[test]
fn rack_decrease_highrtt_uses_the_high_rtt_threshold() {
    let mut bbr = BbrRack::new();
    bbr.gp_min_rtt = 10_000;
    let value = bbr.decrease_highrtt(100, 40_000);
    assert!(value < 100);
    assert!(value > 50);
}

#[test]
fn rack_make_timely_judgement_thresholds() {
    let mut bbr = BbrRack::new();
    bbr.gp_min_rtt = 1_000;
    assert_eq!(bbr.make_timely_judgement(3_000, 0, 1_000), 2);
    assert_eq!(bbr.make_timely_judgement(1_100, 100, 1_000), 0);
    assert_eq!(bbr.make_timely_judgement(2_000, 0, 1_000), 0);
    assert_eq!(bbr.make_timely_judgement(2_000, 100, 1_000), 1);
}

#[test]
fn rack_in_gp_window_reads_the_tcp_flag() {
    let mut stack = stack();
    let bbr = BbrRack::new();
    assert!(!bbr.in_gp_window(&stack));
    stack.tp.t_flags |= TF_GPUTINPROG;
    assert!(bbr.in_gp_window(&stack));
}

#[test]
fn rack_mark_and_clear_gp_marks_toggle_the_flag() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.mark_in_gp_win(&mut stack);
    assert!(bbr.in_gp_window(&stack));
    bbr.clear_gp_marks(&mut stack);
    assert!(!bbr.in_gp_window(&stack));
}

#[test]
fn rack_tend_gp_marks_logs_the_window() {
    let stack = stack();
    let mut bbr = BbrRack::new();
    bbr.gput_ack = 1234;
    bbr.tend_gp_marks(&stack);
    assert_eq!(bbr.log.last().unwrap().fields[0], 1234);
}

#[test]
fn rack_log_gp_calc_records_fields() {
    let mut bbr = BbrRack::new();
    bbr.log_gp_calc(10, 20, 30, 40, 50, 3);
    let event = *bbr.log.last().unwrap();
    assert_eq!(event.kind, 9);
    assert_eq!(event.fields[5], 3);
}

#[test]
fn rack_do_goodput_measurement_sets_the_first_estimate() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.gp_min_rtt = 10_000;
    bbr.pace_min_segs = 1000;
    let sample = GpSample {
        ack: 20_000,
        gput_seq: 0,
        gput_ack: 20_000,
        gput_ts: 0,
        cumack_ts: 0,
        output_ts: 0,
        lowrtt: 10_000,
        high_rwnd: 1_000_000,
        srtt: 10_000,
        prev_srtt: 10_000,
        now: 1_000_000,
        snd_wnd: 65_535,
        filled: false,
        saw_probe_rtt: false,
    };
    let quality = bbr.do_goodput_measurement(&mut stack, sample);
    assert_eq!(quality, Quality::High);
    assert!(bbr.gp_filled);
    assert_eq!(bbr.gp_bw, 20_000);
}

#[test]
fn rack_set_prtt_target_enforces_four_segments() {
    let (stack, mut bbr) = filled();
    bbr.set_prtt_target(&stack, 1000, 10_000, 0);
    assert!(bbr.target_probertt_flight >= 4_000);
}

#[test]
fn rack_enter_probertt_sets_the_state_and_target() {
    let (mut stack, mut bbr) = filled();
    bbr.gp_dyn_mul = true;
    stack.tp.snd_max = 200_000;
    stack.tp.snd_una = 0;
    bbr.enter_probertt(&mut stack, 1000);
    assert!(bbr.in_probe_rtt);
    assert_eq!(bbr.probertt_entered, 1000);
    assert!(bbr.target_probertt_flight >= 4000);
}

#[test]
fn rack_exit_probertt_restores_the_multiplier() {
    let (mut stack, mut bbr) = filled();
    bbr.in_probe_rtt = true;
    bbr.highly_buffered = false;
    bbr.rtt_diff = 50;
    bbr.exit_probertt(&mut stack, 2000);
    assert!(!bbr.in_probe_rtt);
    assert_eq!(bbr.per_gp_ca, 130);
    assert_eq!(bbr.rtt_diff, 0);
}

#[test]
fn rack_check_probe_rtt_enters_after_the_interval() {
    let (mut stack, mut bbr) = filled();
    bbr.gp_dyn_mul = true;
    bbr.probertt_lower = 0;
    bbr.in_probe_rtt = false;
    bbr.skip_timely = false;
    bbr.check_probe_rtt(&mut stack, 96_000_000, 0);
    assert!(bbr.in_probe_rtt);
}

#[test]
fn rack_log_hybrid_records_fields() {
    let stack = stack();
    let mut bbr = BbrRack::new();
    bbr.log_hybrid(&stack, 100, None, 4, 77, 0);
    let event = *bbr.log.last().unwrap();
    assert_eq!(event.kind, 13);
    assert_eq!(event.fields[4], 4);
}

#[test]
fn rack_set_dgp_hybrid_mode_applies_the_cspr_cap() {
    let (mut stack, mut bbr) = filled();
    bbr.hybrid_mode = true;
    let track = SendfileTrack {
        start: 100,
        end: 2000,
        start_seq: 100,
        end_seq: 2000,
        hybrid_flags: HYBRID_WASSET | HYBRID_CSPR,
        cspr: 1_000_000,
        ..SendfileTrack::default()
    };
    bbr.set_dgp_hybrid_mode(&mut stack, 100, 1900, 5000, Some(track));
    assert_eq!(
        bbr.bw_rate_cap,
        bbr.compensate_for_linerate(&stack, 1_000_000)
    );
}

#[test]
fn rack_chk_req_and_hybrid_on_out_extends_and_marks_first_send() {
    let (mut stack, mut bbr) = filled();
    bbr.hybrid_mode = true;
    bbr.last_sft = Some(SendfileTrack {
        flags: TRK_USED,
        start: 100,
        end: 2000,
        start_seq: 100,
        end_seq: 1500,
        ..SendfileTrack::default()
    });
    bbr.chk_req_and_hybrid_on_out(&mut stack, 1000, 1000, 5000, None);
    let track = bbr.last_sft.unwrap();
    assert_eq!(track.end_seq, 2000);
    assert_ne!(track.flags & TRK_FSND, 0);
    assert_eq!(track.first_send, 5000);
}

#[test]
fn rack_set_dgp_sets_the_donor_profile() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    assert!(bbr.set_dgp(&mut stack).is_ok());
    assert!(bbr.dgp_on);
    assert!(bbr.always_pace);
    assert_eq!(bbr.per_gp_ca, 100);
    assert_eq!(bbr.per_gp_rec, 90);
    assert_ne!(stack.t_flags2 & TF2_SUPPORTS_MBUFQ, 0);
}

#[test]
fn rack_clear_rate_sample_clears_the_filter() {
    let mut bbr = BbrRack::new();
    bbr.gp_srtt = 123;
    bbr.gp_rtt_set = true;
    bbr.clear_rate_sample();
    assert_eq!(bbr.gp_srtt, 0);
    assert!(!bbr.gp_rtt_set);
}

#[test]
fn rack_set_pace_segments_uses_the_pacing_len() {
    let (mut stack, mut bbr) = filled();
    bbr.pace_min_segs = 0;
    bbr.pace_max_segs = 0;
    bbr.always_pace = true;
    bbr.gp_bw = 1_000_000;
    assert!(bbr.set_pace_segments(&mut stack, 1, None));
    assert!(bbr.pace_max_segs > 0);
    assert_eq!(bbr.pace_min_segs, 1000);
}

#[test]
fn rack_arrive_at_discounted_rate_halves_in_recovery() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    stack.tp.t_flags |= TF_FASTRECOVERY;
    stack.options.rack_hibeta = false;
    let (reduced, rate_set, gain) = bbr.arrive_at_discounted_rate(&stack, 100_000, false);
    assert_eq!(reduced, 50_000);
    assert!(!rate_set);
    assert_eq!(gain, 50);
}

#[test]
fn pace_to_fill_cwnd_uses_the_last_rtt() {
    let (stack, mut bbr) = filled();
    bbr.last_us_rtt = 10_000;
    bbr.cwnd_to_use = 100_000;
    bbr.fillcw_cap = 0;
    bbr.tun.bw_multipler = 0;
    let mut rate = 1_000_000;
    let delay = bbr.pace_to_fill_cwnd(&stack, 5, 1000, 1000, &mut rate, true);
    assert!(delay > 0);
    assert!(bbr.via_fill_cw);
    assert!(rate > 0);
}

#[test]
fn rack_log_pacing_delay_calc_records_fields() {
    let mut bbr = BbrRack::new();
    bbr.log_pacing_delay_calc(1000, 2000, 3000, 4000, 5000, 77, 2);
    let event = *bbr.log.last().unwrap();
    assert_eq!(event.kind, 14);
    assert_eq!(event.fields[1], 2000);
}

#[test]
fn rack_get_pacing_delay_old_method_is_positive() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.always_pace = false;
    stack.tp.t_srtt = 10_000;
    stack.tp.snd_cwnd = 10_000;
    let delay = bbr.get_pacing_delay(&mut stack, 1000, false, 1000, 0);
    assert_eq!(delay, 1000);
}

#[test]
fn rack_start_gp_measurement_sets_the_initial_window() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    stack.tp.t_state = 4;
    stack.tp.snd_una = 0;
    stack.tp.snd_max = 100_000;
    bbr.start_gp_measurement(&mut stack, 0, 0, 1_000, 100_000, 1_000);
    assert_ne!(stack.tp.t_flags & TF_GPUTINPROG, 0);
    assert_eq!(bbr.gput_ack, 10_000);
}

#[test]
fn rack_get_hpts_pacing_min_for_bw_rounds_to_mss() {
    let mut bbr = BbrRack::new();
    bbr.gp_bw = 1_000_000;
    assert_eq!(bbr.get_hpts_pacing_min_for_bw(1000, 1000), 1000);
}

#[test]
fn program_hybrid_pacing_installs_the_request() {
    let (mut stack, mut bbr) = filled();
    let request = HybridRequest {
        start: 100,
        end: 2000,
        cspr: 1_000_000,
        hint_maxseg: 1200,
        hybrid_flags: HYBRID_ENABLE | HYBRID_CSPR | HYBRID_H_MS | HYBRID_CU,
    };
    assert!(bbr.process_hybrid_pacing(&mut stack, request, 5000).is_ok());
    assert!(bbr.hybrid_mode);
    assert!(bbr.catch_up);
    let track = bbr.last_sft.unwrap();
    assert_eq!(track.cspr, 1_000_000);
    assert_eq!(track.hint_maxseg, 1200);
    assert!(track.deadline > 5000);
}

#[test]
fn rack_gain_for_fastoutput_expands_within_gating() {
    let mut stack = stack();
    stack.tp.snd_una = 150_000;
    stack.tp.snd_max = 200_000;
    let bbr = BbrRack::new();
    // outstanding 50,000; sb_avail 80,000 gives 30,000 of room; wnd gives 15,535.
    assert_eq!(
        bbr.gain_for_fastoutput(&stack, 10_000, 1_000, 80_000),
        11_000
    );
    assert_eq!(
        bbr.gain_for_fastoutput(&stack, 15_000, 1_000, 80_000),
        15_000
    );
}
