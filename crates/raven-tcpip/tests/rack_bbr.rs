// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K328 integration vectors for the RACK BBR/pacing donor functions.

use raven_tcpip::{
    BbrRack, GpSample, HYBRID_CSPR, HYBRID_CU, HYBRID_ENABLE, HybridRequest, Quality, RACK_REQ_AVG,
    RackStack,
};

fn stack() -> RackStack {
    let mut stack = RackStack::new();
    stack.tp.t_maxseg = 1000;
    stack.tp.t_srtt = 10_000;
    stack.tp.snd_max = 1_000_000;
    stack.tp.snd_una = 0;
    stack.tp.snd_nxt = 500_000;
    stack.tp.snd_cwnd = 100_000;
    stack.tp.snd_ssthresh = 1_000_000;
    stack.snd_wnd = 65_535;
    stack.bytes_transmitted = 500_000;
    stack.bytes_retransmitted = 1000;
    stack
}

fn sample(ack: u32, now: u64, filled: bool) -> GpSample {
    GpSample {
        ack,
        gput_seq: 0,
        gput_ack: 60_000,
        gput_ts: 0,
        cumack_ts: 0,
        output_ts: 0,
        lowrtt: 10_000,
        high_rwnd: 1_000_000,
        srtt: 10_000,
        prev_srtt: 10_000,
        now,
        snd_wnd: 65_535,
        filled,
        saw_probe_rtt: false,
    }
}

#[test]
fn goodput_measurements_average_then_track_the_filter() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.gp_min_rtt = 10_000;
    bbr.pace_min_segs = 1000;
    for index in 1..=4u32 {
        let quality = bbr.do_goodput_measurement(
            &mut stack,
            sample(60_000, u64::from(index) * 1_000_000, index > 1),
        );
        assert_eq!(quality, Quality::High);
    }
    assert!(bbr.gp_filled);
    assert!(bbr.num_measurements >= RACK_REQ_AVG);
    assert!(bbr.gp_bw > 0);
    assert!(!bbr.log.is_empty());
}

#[test]
fn probe_rtt_round_trip_restores_the_multipliers() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.gp_filled = true;
    bbr.gp_bw = 1_000_000;
    bbr.gp_srtt = 10_000;
    bbr.gp_min_rtt = 10_000;
    bbr.pace_min_segs = 1000;
    bbr.gp_dyn_mul = true;
    bbr.per_gp_ca = 100;
    bbr.per_gp_ss = 100;
    bbr.enter_probertt(&mut stack, 1_000);
    assert!(bbr.in_probe_rtt);
    assert!(bbr.target_probertt_flight >= 4_000);
    bbr.exit_probertt(&mut stack, 2_000);
    assert!(!bbr.in_probe_rtt);
    assert_eq!(bbr.per_gp_ca, 130);
    assert_eq!(bbr.rtt_diff, 0);
}

#[test]
fn hybrid_catch_up_sets_a_deadline_and_cap() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.gp_filled = true;
    bbr.gp_bw = 1_000_000;
    bbr.pace_min_segs = 1000;
    bbr.gp_ready = true;
    let request = HybridRequest {
        start: 100,
        end: 4100,
        cspr: 1_000_000,
        hint_maxseg: 1200,
        hybrid_flags: HYBRID_ENABLE | HYBRID_CSPR | HYBRID_CU,
    };
    bbr.process_hybrid_pacing(&mut stack, request, 10_000)
        .unwrap();
    assert!(bbr.hybrid_mode);
    assert!(bbr.catch_up);
    assert!(bbr.bw_rate_cap > 0);
    assert!(bbr.last_sft.unwrap().deadline > 10_000);
}

#[test]
fn timely_increase_and_decrease_move_the_multipliers() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.gp_min_rtt = 10_000;
    bbr.gp_saw_ss = true;
    bbr.per_gp_ss = 50;
    bbr.increase_bw_mul(&stack, 0, 1_000_000, 1_000_000, false);
    assert_eq!(bbr.per_gp_ss, 52);
    bbr.gp_saw_ss = false;
    bbr.gp_saw_ca = true;
    bbr.per_gp_ca = 100;
    bbr.decrease_bw_mul(1, 40_000, 10_000);
    assert_eq!(bbr.per_gp_ca, 50);
    stack.tp.t_flags |= raven_tcpip::TF_FASTRECOVERY;
}

#[test]
fn pacing_delay_old_and_paced_paths_are_positive() {
    let mut stack = stack();
    let mut bbr = BbrRack::new();
    bbr.always_pace = false;
    assert!(bbr.get_pacing_delay(&mut stack, 20_000, false, 1000, 0) > 0);
    bbr.always_pace = true;
    bbr.gp_filled = true;
    bbr.gp_ready = true;
    bbr.gp_bw = 1_000_000;
    bbr.num_measurements = RACK_REQ_AVG;
    bbr.pace_min_segs = 1000;
    bbr.dgp_on = true;
    assert!(bbr.get_pacing_delay(&mut stack, 1000, false, 1000, 1_000_000) > 0);
}
