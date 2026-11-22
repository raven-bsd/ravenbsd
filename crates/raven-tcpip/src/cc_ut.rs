// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD CUBIC port tests.
//!
//! Expected values are computed from the donor `cc_cubic.h`/`cc_cubic.c` arithmetic with
//! `CUBIC_SHIFT=8`, `CUBIC_BETA=179`, `CUBIC_FC=217`, `CUBIC_C_FACTOR=102`.

use super::*;

const MSS: u32 = 1_460;

fn context() -> CubicContext {
    CubicContext {
        mss: MSS,
        srtt_ms: 100,
        rtt_updated: 8,
        nsegs: 1,
        cwnd_limited: true,
        ..CubicContext::default()
    }
}

fn cubic() -> Cubic {
    Cubic::new(MSS, 420_480, 294_336, 1_000_000)
}

#[test]
fn cubic_k_matches_the_donor_reference() {
    assert_eq!(Cubic::cubic_k(288, 200), 1_552);
    assert_eq!(Cubic::cubic_k(288, 288), 0);
    assert_eq!(Cubic::cubic_k(1000, 288), 3_120);
    let mut c = cubic();
    c.k = Cubic::cubic_k(288, 200);
    assert_eq!(c.k_us(), 6_062_500);
}

#[test]
fn cubic_cwnd_matches_the_donor_reference() {
    let k = Cubic::cubic_k(288, 200);
    let at_epoch = (((k as u128) * 1_000_000) >> CUBIC_SHIFT) as i64;
    assert_eq!(Cubic::cubic_cwnd(0, 420_480, MSS, k), 290_540);
    assert_eq!(Cubic::cubic_cwnd(at_epoch, 420_480, MSS, k), 420_480);
    assert_eq!(Cubic::cubic_cwnd(2 * at_epoch, 420_480, MSS, k), 548_960);
}

#[test]
fn cubic_slow_start_uses_abc_and_nsegs() {
    let mut c = Cubic::new(MSS, 14_600, 1_000_000, 1_000_000);
    let mut ctx = context();
    ctx.acked = 2 * MSS;
    ctx.nsegs = 2;
    ctx.snd_nxt = 100_000;
    ctx.snd_max = 100_000;
    ctx.cwnd = 14_600;
    c.ack_received(&ctx);
    assert_eq!(c.cwnd(), 14_600 + 2 * MSS);

    let mut c = Cubic::new(MSS, 14_600, 1_000_000, 1_000_000);
    let mut ctx = context();
    ctx.acked = 2 * MSS;
    ctx.nsegs = 2;
    ctx.snd_nxt = 100_000;
    ctx.snd_max = 200_000;
    ctx.cwnd = 14_600;
    c.ack_received(&ctx);
    assert_eq!(c.cwnd(), 14_600 + MSS);
}

#[test]
fn cubic_slow_start_waits_for_eight_rtt_samples() {
    let mut c = Cubic::new(MSS, 14_600, 1_000_000, 1_000_000);
    let mut ctx = context();
    ctx.rtt_updated = 7;
    ctx.acked = MSS;
    ctx.snd_nxt = 100_000;
    ctx.snd_max = 100_000;
    ctx.cwnd = 14_600;
    c.ack_received(&ctx);
    assert_eq!(c.min_rtt_us(), 0);
    assert_eq!(c.cwnd(), 14_600 + MSS);
}

#[test]
fn cubic_record_rtt_tracks_minimum_and_epoch_sum() {
    let mut c = cubic();
    let mut ctx = context();
    ctx.rtt_updated = 7;
    ctx.srtt_ms = 50;
    c.record_rtt(&ctx);
    assert_eq!(c.min_rtt_us(), 0);

    ctx.rtt_updated = 8;
    ctx.srtt_ms = 100;
    c.record_rtt(&ctx);
    assert_eq!(c.min_rtt_us(), 100_000);
    assert_eq!(c.mean_rtt_us(), 100_000);

    ctx.srtt_ms = 50;
    c.record_rtt(&ctx);
    assert_eq!(c.min_rtt_us(), 50_000);
    assert_eq!(c.mean_rtt_us(), 100_000);
    assert_eq!(c.epoch_ack_count, 2);
    assert_eq!(c.sum_rtt_us, 150_000);
}

#[test]
fn cubic_get_ssthresh_applies_fast_convergence() {
    let mut c = cubic();
    c.cwnd = 300_000;
    c.w_max = 356_422;
    let mut ctx = context();
    ctx.pipe = 400_000;
    assert_eq!(c.get_ssthresh(&ctx), 279_687);
    assert_eq!(c.w_max(), (300_000 * 217) >> 8);
}

#[test]
fn cubic_get_ssthresh_uses_newreno_inside_tf() {
    let mut c = cubic();
    c.cwnd = 420_480;
    c.w_max = 400_000;
    c.flags |= CUBICFLAG_IN_TF;
    let ctx = context();
    assert_eq!(c.get_ssthresh(&ctx), 210_240);
    assert_eq!(c.w_max(), 420_480);
}

#[test]
fn cubic_cong_signal_dupack_cuts_by_beta_and_marks_cong_event() {
    let mut c = cubic();
    let mut ctx = context();
    ctx.pipe = 400_000;
    ctx.cwnd = 420_480;
    c.congestion(&ctx, CubicSignal::DupAck);
    assert_eq!(c.ssthresh(), 279_687);
    assert_eq!(c.w_max(), 356_422);
    assert!(c.flags() & CUBICFLAG_CONG_EVENT != 0);
    assert!(c.in_recovery());
}

#[test]
fn cubic_cong_signal_ecn_sets_cwnd_to_ssthresh() {
    let mut c = cubic();
    let mut ctx = context();
    ctx.pipe = 400_000;
    ctx.cwnd = 420_480;
    c.congestion(&ctx, CubicSignal::Ecn);
    assert_eq!(c.ssthresh(), 279_687);
    assert_eq!(c.cwnd(), 279_687);
    assert!(c.flags() & CUBICFLAG_CONG_EVENT != 0);
    assert!(c.in_recovery());
}

#[test]
fn cubic_cong_signal_rto_uses_beta_of_the_min_send_window() {
    let mut c = cubic();
    let mut ctx = context();
    ctx.pipe = 300_000;
    ctx.snd_wnd = 200_000;
    ctx.rxtshift = 1;
    c.congestion(&ctx, CubicSignal::Rto);
    assert_eq!(c.ssthresh(), 138_700);
    assert_eq!(c.cwnd(), MSS);
    assert!(c.flags() & CUBICFLAG_RTO_EVENT != 0);

    ctx.rxtshift = 2;
    c.congestion(&ctx, CubicSignal::Rto);
    assert_eq!(c.ssthresh(), 138_700);
    assert_eq!(c.cwnd(), MSS);
}

#[test]
fn cubic_rto_error_restores_the_saved_epoch() {
    let mut c = cubic();
    c.k = 1_552;
    c.w_max = 420_480;
    c.cwnd_epoch = 400_000;
    c.t_epoch = 99;
    let mut ctx = context();
    ctx.pipe = 300_000;
    ctx.snd_wnd = 200_000;
    ctx.rxtshift = 1;
    c.congestion(&ctx, CubicSignal::Rto);
    c.congestion(&ctx, CubicSignal::RtoErr);
    assert_eq!(c.k(), 1_552);
    assert_eq!(c.w_max(), 420_480);
    assert_eq!(c.cwnd_epoch, 400_000);
    assert_eq!(c.t_epoch, 99);
    assert!(c.flags() & CUBICFLAG_RTO_EVENT == 0);
}

#[test]
fn cubic_post_recovery_uses_the_pipe_floor_then_beta() {
    let mut c = cubic();
    c.in_recovery = true;
    c.cwnd = 294_336;
    c.ssthresh = 200_000;
    c.w_max = 420_480;
    let mut ctx = context();
    ctx.pipe = 100_000;
    c.post_recovery(&ctx);
    assert_eq!(c.cwnd(), 101_460);

    let mut c = cubic();
    c.in_recovery = true;
    c.cwnd = 294_336;
    c.ssthresh = 200_000;
    c.w_max = 420_480;
    let mut ctx = context();
    ctx.pipe = 250_000;
    c.post_recovery(&ctx);
    assert_eq!(c.cwnd(), 294_007);
}

#[test]
fn cubic_post_recovery_averages_the_epoch_rtt_and_resets() {
    let mut c = cubic();
    c.epoch_ack_count = 10;
    c.sum_rtt_us = 1_000_000;
    let mut ctx = context();
    ctx.pipe = 250_000;
    c.post_recovery(&ctx);
    assert_eq!(c.mean_rtt_us(), 100_000);
    assert_eq!(c.epoch_ack_count, 0);
    assert_eq!(c.sum_rtt_us, 0);
}

#[test]
fn cubic_after_idle_resets_the_restart_window_and_epoch() {
    let mut c = cubic();
    c.cwnd = 100 * MSS;
    c.ssthresh = 1_000_000;
    c.w_max = 200_000;
    c.cwnd_epoch = 100_000;
    c.after_idle(12_345);
    assert_eq!(c.cwnd(), 10 * MSS);
    assert_eq!(c.ssthresh(), 1_000_000);
    assert_eq!(c.w_max(), 200_000);
    assert_eq!(c.k(), Cubic::cubic_k(136, 68));
    assert_eq!(c.t_epoch, 12_345);
    assert!(c.flags() & CUBICFLAG_HYSTART_ENABLED != 0);
}

#[test]
fn cubic_newround_rotates_the_css_round() {
    let mut c = cubic();
    c.css_current_round_minrtt = 10_000;
    c.css_lastround_minrtt = CSS_INFINITY;
    c.css_rttsample_count = 5;
    c.newround(4, false, false);
    assert_eq!(c.css_lastround_minrtt, 10_000);
    assert_eq!(c.css_current_round_minrtt, CSS_INFINITY);
    assert_eq!(c.css_rttsample_count, 0);
    assert_eq!(c.css_current_round, 4);
}

#[test]
fn cubic_newround_leaves_css_after_five_rounds() {
    let mut c = cubic();
    c.cwnd = 50_000;
    c.css_lowrtt_fas = 55_000;
    c.css_fas_at_css_entry = 60_000;
    c.css_entered_at_round = 1;
    c.css_current_round = 6;
    c.flags |= CUBICFLAG_HYSTART_IN_CSS;
    c.newround(6, false, false);
    assert_eq!(c.ssthresh(), 50_000);
    assert!(c.flags() & CUBICFLAG_HYSTART_IN_CSS == 0);
    assert!(c.flags() & CUBICFLAG_HYSTART_ENABLED == 0);
}

#[test]
fn cubic_rttsample_tracks_the_round_minimum() {
    let mut c = cubic();
    c.rttsample(5_000, 2, 1);
    assert_eq!(c.css_rttsample_count, 0);
    c.rttsample(10_000, 1, 50);
    assert_eq!(c.css_current_round_minrtt, 10_000);
    assert_eq!(c.css_lowrtt_fas, 50);
    c.rttsample(9_000, 1, 55);
    assert_eq!(c.css_current_round_minrtt, 9_000);
    assert_eq!(c.css_lowrtt_fas, 55);
    assert_eq!(c.css_rttsample_count, 2);
}

#[test]
fn cubic_ack_received_resets_the_epoch_then_grows_on_the_curve() {
    let mut c = cubic();
    c.cwnd = 420_480;
    c.w_max = 420_480;
    c.cwnd_epoch = 420_480;
    c.k = 0;
    c.min_rtt_us = 100_000;
    c.mean_rtt_us = 100_000;
    c.flags = CUBICFLAG_CONG_EVENT;
    c.rtt_updated = 8;
    let mut ctx = context();
    ctx.cwnd = 420_480;
    ctx.acked = MSS;
    ctx.snd_nxt = 100_000;
    ctx.snd_max = 100_000;
    ctx.now_us = 0;
    c.ack_received(&ctx);
    assert_eq!(c.k(), 0);
    assert_eq!(c.cwnd(), 420_480);

    ctx.now_us = 2_000_000;
    c.ack_received(&ctx);
    assert_eq!(c.cwnd(), 420_491);
}

#[test]
fn cubic_ack_received_uses_the_reno_friendly_window() {
    let mut c = cubic();
    c.cwnd = 420_480;
    c.w_max = 290 * MSS;
    c.cwnd_epoch = 420_480;
    c.k = Cubic::cubic_k(290, 420_480 / MSS);
    c.min_rtt_us = 100_000;
    c.mean_rtt_us = 100_000;
    c.flags = CUBICFLAG_CONG_EVENT;
    c.rtt_updated = 8;
    c.abc_bytes = c.cwnd;
    let mut ctx = context();
    ctx.cwnd = 420_480;
    ctx.acked = MSS;
    ctx.snd_nxt = 100_000;
    ctx.snd_max = 100_000;
    ctx.now_us = 0;
    c.ack_received(&ctx);
    assert_eq!(c.cwnd(), 420_480 + MSS);
    assert!(c.flags() & CUBICFLAG_IN_TF != 0);
}

#[test]
fn cubic_on_rto_exits_recovery_like_the_timer() {
    let mut c = cubic();
    c.in_recovery = true;
    let mut ctx = CcContext {
        mss: MSS,
        flight: 100_000,
        pipe: 100_000,
        ..CcContext::default()
    };
    c.on_rto(&mut ctx);
    assert!(!c.in_recovery());
    assert_eq!(c.cwnd(), MSS);
}

#[test]
fn cubic_initial_state_is_the_donor_connection_init() {
    let c = cubic();
    assert_eq!(c.cwnd(), 420_480);
    assert_eq!(c.ssthresh(), 294_336);
    assert_eq!(c.w_max(), u32::MAX);
    assert_eq!(c.min_rtt_us(), 0);
    assert_eq!(c.mean_rtt_us(), 1);
    assert!(c.flags() & CUBICFLAG_HYSTART_ENABLED != 0);
}
