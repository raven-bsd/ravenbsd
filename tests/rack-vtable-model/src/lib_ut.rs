// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Reference, lifecycle, limit and randomized trace tests for the RACK vtable model.

use std::collections::BTreeSet;

use crate::model::{
    CtlReply, EAGAIN, EINVAL, EOPNOTSUPP, Event, EventResult, PACE_PKT_OUTPUT, PACE_TMR_MASK,
    PACE_TMR_RACK, PRUS_OOB, Query, RackOption, RackOptions, SegmentKind, SockoptDir, StackInfo,
    TF2_MBUF_ACKCMP, TF2_SUPPORTS_MBUFQ, TcpState, VtableModel,
};
use crate::rng::Rng;

const CALLBACKS: [&str; 19] = [
    "tfb_tcp_output",
    "tfb_tcp_do_segment",
    "tfb_do_segment_nounlock",
    "tfb_do_queued_segments",
    "tfb_tcp_ctloutput",
    "tfb_tcp_fb_init",
    "tfb_tcp_fb_fini",
    "tfb_tcp_timer_stop_all",
    "tfb_tcp_rexmit_tmr",
    "tfb_tcp_handoff_ok",
    "tfb_tcp_mtu_chg",
    "tfb_pru_options",
    "tfb_hwtls_change",
    "tfb_chg_query",
    "tfb_switch_failed",
    "tfb_early_wake_check",
    "tfb_compute_pipe",
    "tfb_stack_info",
    "tfb_inherit",
];

#[test]
fn init_fini_lifecycle_and_registration() {
    let mut model = VtableModel::new();
    assert_eq!(model.refcnt, 0);
    assert!(!model.fb_ptr);
    assert_eq!(model.flags, 0x06);

    assert_eq!(model.output(10), EINVAL);
    assert_eq!(model.timer_stop_all(), EINVAL);
    assert_eq!(
        model
            .ctloutput(SockoptDir::Get, RackOption::PacingDivisor, 0)
            .error,
        EINVAL
    );
    assert_eq!(model.chg_query(Query::SendMap).error, EINVAL);
    assert_eq!(model.compute_pipe(), -EINVAL);
    assert_eq!(model.stack_info().error, EINVAL);

    assert_eq!(model.init(), 0);
    assert!(model.fb_ptr);
    assert!(model.initialized);
    assert_eq!(model.init(), EINVAL);

    assert_eq!(model.register(), 0);
    assert_eq!(model.register(), 0);
    assert_eq!(model.refcnt, 2);
    assert_eq!(model.deregister(), 0);
    assert_eq!(model.deregister(), 0);
    assert_eq!(model.deregister(), EINVAL);

    model.hwtls_change(true);
    model.hpts_flags = PACE_TMR_RACK;
    model.in_hpts = true;
    model.fini(true);
    assert!(!model.fb_ptr);
    assert!(!model.initialized);
    assert!(!model.hw_tls);
    assert_eq!(model.hpts_flags, 0);
    assert!(!model.in_hpts);
    assert!(model.timers_stopped);
    assert!(model.purged);
    assert_eq!(model.output(10), EINVAL);

    assert_eq!(model.init(), 0);
    assert!(model.fb_ptr);
    model.fini(false);
}

#[test]
fn output_retransmits_then_sends_at_snd_max_and_arms_pacing() {
    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    model.now_us = 1_000;
    model.snd_una = 1_000;
    model.snd_max = 3_000;
    model.snd_nxt = 2_500;
    model.must_retran = true;

    assert_eq!(model.output(500), 0);
    assert_eq!(model.bytes_retransmitted, 2_000);
    assert_eq!(model.bytes_transmitted, 500);
    assert!(!model.must_retran);
    assert_eq!(model.snd_max, 3_500);
    assert_eq!(model.snd_nxt, 3_500);
    assert_eq!(model.hpts_flags & PACE_PKT_OUTPUT, PACE_PKT_OUTPUT);
    assert!(model.in_hpts);
    assert_eq!(model.last_output_to_us, 1_100);

    assert_eq!(model.output(0), 0);
    assert_eq!(model.bytes_transmitted, 500);
    assert_eq!(model.snd_max, 3_500);
}

#[test]
fn segment_order_returns_and_queue_drain() {
    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    model.queued = 3;
    model.snd_una = 100;
    model.snd_max = 500;
    model.hpts_flags = PACE_TMR_RACK | PACE_PKT_OUTPUT;
    model.in_hpts = true;

    assert_eq!(model.do_segment(SegmentKind::Ack, 400), 0);
    assert_eq!(model.queued, 0);
    assert_eq!(model.snd_una, 500);
    assert_eq!(model.hpts_flags, 0);
    assert!(!model.in_hpts);

    model.now_us = 5_000;
    assert_eq!(model.do_segment_nounlock(SegmentKind::Data, 0), 0);
    assert_eq!(model.rcvtime_us, 5_000);

    assert_eq!(model.do_segment(SegmentKind::Fin, 0), 0);
    assert_eq!(model.state, TcpState::CloseWait);

    model.hpts_flags = PACE_TMR_RACK;
    model.in_hpts = true;
    assert_eq!(model.do_segment(SegmentKind::Rst, 0), 0);
    assert_eq!(model.state, TcpState::Closed);
    assert_eq!(model.hpts_flags, 0);
    assert!(!model.in_hpts);

    assert_eq!(model.do_queued_segments(true), 1);
    assert!(model.lost_tcb);
    assert_eq!(model.queued, 0);
    assert_eq!(model.do_queued_segments(false), 0);
}

#[test]
fn handoff_matches_the_donor_state_rules() {
    let mut model = VtableModel::new();
    assert_eq!(model.handoff_ok(), EINVAL);
    assert_eq!(model.init(), 0);

    model.state = TcpState::Closed;
    assert_eq!(model.handoff_ok(), 0);
    model.state = TcpState::Listen;
    assert_eq!(model.handoff_ok(), 0);
    model.state = TcpState::SynSent;
    assert_eq!(model.handoff_ok(), EAGAIN);
    model.state = TcpState::SynReceived;
    assert_eq!(model.handoff_ok(), EAGAIN);

    model.state = TcpState::Established;
    model.sack_permit = false;
    model.sack_not_required = false;
    assert_eq!(model.handoff_ok(), EINVAL);
    model.sack_permit = true;
    assert_eq!(model.handoff_ok(), 0);
    model.sack_permit = false;
    model.sack_not_required = true;
    assert_eq!(model.handoff_ok(), 0);

    model.sent_fin = true;
    model.snd_max = 100;
    model.snd_una = 0;
    assert_eq!(model.handoff_ok(), EAGAIN);
    model.snd_una = 99;
    assert_eq!(model.handoff_ok(), 0);
}

#[test]
fn timer_order_clears_the_scoreboard_only_after_three_timeouts() {
    let mut model = VtableModel::new();
    assert_eq!(model.timer_stop_all(), EINVAL);
    assert_eq!(model.init(), 0);
    model.sacked = 100;
    model.considered_lost = 20;
    model.holes_rxt = 5;
    model.hpts_flags = PACE_TMR_RACK;
    model.in_hpts = true;

    model.rexmit_timer();
    assert_eq!(model.rxtshift, 1);
    assert!(model.must_retran);
    assert!(!model.scoreboard_cleared);
    assert_eq!(model.sacked, 100);
    assert!(!model.in_hpts);
    assert_eq!(model.hpts_flags & PACE_TMR_RACK, 0);

    model.rexmit_timer();
    assert_eq!(model.rxtshift, 2);
    assert!(!model.scoreboard_cleared);

    model.rexmit_timer();
    assert_eq!(model.rxtshift, 3);
    assert!(model.scoreboard_cleared);
    assert_eq!(model.sacked, 0);
    assert_eq!(model.considered_lost, 0);
    assert_eq!(model.holes_rxt, 0);

    model.hpts_flags = PACE_TMR_RACK;
    model.in_hpts = true;
    assert_eq!(model.timer_stop_all(), 0);
    assert!(model.timers_stopped);
    assert_eq!(model.hpts_flags, 0);
    assert!(!model.in_hpts);
}

#[test]
fn mtu_change_retransmits_and_snaps_snd_nxt() {
    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    model.options.pace_min_segs = 1_460;
    model.snd_max = 1_000;
    model.snd_nxt = 500;

    model.mtu_change(1_200);
    assert_eq!(model.options.pace_min_segs, 1_200);
    assert_eq!(model.rxtshift, 1);
    assert!(model.must_retran);
    assert_eq!(model.snd_nxt, 1_000);

    model.mtu_change(1_200);
    assert_eq!(model.rxtshift, 1);
    model.mtu_change(1_000);
    assert_eq!(model.rxtshift, 2);
}

#[test]
fn ctloutput_limits_and_get_set() {
    let mut model = VtableModel::new();
    assert_eq!(
        model
            .ctloutput(SockoptDir::Set, RackOption::PacingDivisor, 0)
            .error,
        EINVAL
    );
    assert_eq!(model.init(), 0);

    assert_eq!(
        model
            .ctloutput(SockoptDir::Set, RackOption::PacingDivisor, 10)
            .error,
        0
    );
    assert_eq!(
        model
            .ctloutput(SockoptDir::Get, RackOption::PacingDivisor, 0)
            .value,
        Some(50)
    );
    model.ctloutput(SockoptDir::Set, RackOption::PacingDivisor, 0);
    assert_eq!(
        model
            .ctloutput(SockoptDir::Get, RackOption::PacingDivisor, 0)
            .value,
        Some(250)
    );
    model.ctloutput(SockoptDir::Set, RackOption::PacingDivisor, 100);
    assert_eq!(
        model
            .ctloutput(SockoptDir::Get, RackOption::PacingDivisor, 0)
            .value,
        Some(100)
    );

    model.ctloutput(SockoptDir::Set, RackOption::PaceMinSeg, 0x1_2345);
    assert_eq!(
        model
            .ctloutput(SockoptDir::Get, RackOption::PaceMinSeg, 0)
            .value,
        Some(0x2345)
    );
    model.ctloutput(SockoptDir::Set, RackOption::PaceMaxSeg, 1_460);
    assert_eq!(
        model
            .ctloutput(SockoptDir::Get, RackOption::PaceMaxSeg, 0)
            .value,
        Some(1_460)
    );

    assert_eq!(
        model
            .ctloutput(SockoptDir::Set, RackOption::PacingBeta, 60)
            .error,
        EINVAL
    );
    assert_eq!(
        model
            .ctloutput(SockoptDir::Get, RackOption::PacingBeta, 0)
            .value,
        Some(50)
    );
    assert_eq!(
        model
            .ctloutput(SockoptDir::Set, RackOption::PacingBetaEcn, 70)
            .error,
        0
    );
    assert_eq!(
        model
            .ctloutput(SockoptDir::Get, RackOption::PacingBetaEcn, 0)
            .value,
        Some(70)
    );
    model.options.newreno = false;
    assert_eq!(
        model
            .ctloutput(SockoptDir::Set, RackOption::PacingBetaEcn, 80)
            .error,
        EINVAL
    );
    assert_eq!(
        model
            .ctloutput(SockoptDir::Get, RackOption::PacingBetaEcn, 0)
            .error,
        EINVAL
    );

    assert_eq!(
        model
            .ctloutput(SockoptDir::Set, RackOption::Unknown(999), 0)
            .error,
        EOPNOTSUPP
    );
    assert_eq!(
        model
            .ctloutput(SockoptDir::Get, RackOption::Unknown(999), 0)
            .error,
        EOPNOTSUPP
    );
}

#[test]
fn query_pipe_stack_info_and_pru_options() {
    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    assert_eq!(model.chg_query(Query::TimersUp).value, 0);
    model.hpts_flags = PACE_TMR_RACK;
    model.timer_exp_us = 500;
    let timers = model.chg_query(Query::TimersUp);
    assert_eq!(timers.value, 1);
    assert_eq!(timers.timer_flags, PACE_TMR_RACK);
    assert_eq!(timers.timer_exp_us, 500);

    model.snd_max = 1_000;
    model.snd_una = 200;
    model.sacked = 100;
    model.considered_lost = 50;
    model.holes_rxt = 75;
    assert_eq!(model.compute_pipe(), 725);
    let rack = model.chg_query(Query::RackTimes);
    assert_eq!(rack.value, 1);
    assert_eq!(rack.sacked, 100);
    assert_eq!(rack.considered_lost, 50);
    assert_eq!(rack.holes_rxt, 75);
    assert_eq!(model.chg_query(Query::SendMap).value, 0);
    assert_eq!(model.chg_query(Query::Unknown(99)).error, -EINVAL);

    model.bytes_transmitted = 1_234;
    model.bytes_retransmitted = 56;
    assert_eq!(
        model.stack_info(),
        StackInfo {
            error: 0,
            bytes_transmitted: 1_234,
            bytes_retransmitted: 56,
        }
    );

    assert_eq!(model.pru_options(PRUS_OOB), EOPNOTSUPP);
    assert_eq!(model.pru_options(0), 0);
    assert_eq!(model.pru_options(PRUS_OOB | 0x10), EOPNOTSUPP);
}

#[test]
fn inherit_hwtls_and_stable_trace_line() {
    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    model.hwtls_change(true);
    assert_eq!(
        model.trace_line(7, "tfb_hwtls_change", "enabled=true", 0),
        "7|tfb_hwtls_change|enabled=true|0|1|1|0|0x00|0|0|0|0|0|0|0|0|0"
    );

    let source = RackOptions {
        pace_min_segs: 1_200,
        pace_max_segs: 1_460,
        pacing_divisor: 100,
        beta: 60,
        beta_ecn: 70,
        newreno: true,
        flags2: TF2_SUPPORTS_MBUFQ | TF2_MBUF_ACKCMP,
    };
    model.inherit(&source, false);
    assert_ne!(model.options, source);
    model.inherit(&source, true);
    assert_eq!(model.options, source);
    assert_eq!(
        model.flags2 & (TF2_SUPPORTS_MBUFQ | TF2_MBUF_ACKCMP),
        TF2_SUPPORTS_MBUFQ | TF2_MBUF_ACKCMP
    );

    model.hwtls_change(false);
    model.fini(false);
    assert!(!model.hw_tls);
    assert!(!model.fb_ptr);
    assert_eq!(
        model.ctloutput(SockoptDir::Get, RackOption::PacingDivisor, 0),
        CtlReply {
            error: EINVAL,
            value: None,
        }
    );
}

#[test]
fn switch_failed_and_early_wake_follow_the_timer_flags() {
    let mut model = VtableModel::new();
    assert_eq!(model.init(), 0);
    model.state = TcpState::Established;
    model.queue_ready = true;
    model.pacing = false;
    model.now_us = 1_000;

    model.switch_failed();
    assert_eq!(
        model.flags2 & (TF2_SUPPORTS_MBUFQ | TF2_MBUF_ACKCMP),
        TF2_SUPPORTS_MBUFQ | TF2_MBUF_ACKCMP
    );
    assert!(model.in_hpts);
    assert_eq!(model.hpts_flags & PACE_TMR_RACK, PACE_TMR_RACK);
    assert_eq!(model.timer_exp_us, 1_010);
    assert!(!model.early_wake_check());
    model.now_us = 1_010;
    assert!(model.early_wake_check());

    model.in_hpts = false;
    model.hpts_flags = PACE_PKT_OUTPUT;
    model.last_output_to_us = 2_000;
    model.now_us = 1_999;
    assert!(!model.early_wake_check());
    model.now_us = 2_000;
    assert!(model.early_wake_check());
}

#[test]
fn all_nineteen_callbacks_have_a_trace_line() {
    let mut model = VtableModel::new();
    let events = [
        Event::Init,
        Event::Fini(false),
        Event::Init,
        Event::Output(100),
        Event::Segment(SegmentKind::Ack, 10),
        Event::SegmentUnlock(SegmentKind::Data, 0),
        Event::Queued(false),
        Event::Ctl(SockoptDir::Get, RackOption::PacingDivisor, 0),
        Event::TimerStop,
        Event::Rexmit,
        Event::Handoff,
        Event::Mtu(1_200),
        Event::Pru(0),
        Event::Hwtls(true),
        Event::Query(Query::TimersUp),
        Event::Switch,
        Event::Wake,
        Event::Pipe,
        Event::Info,
        Event::Inherit(true),
    ];
    let mut seen = BTreeSet::new();
    for (seq, event) in events.iter().copied().enumerate() {
        let result = event.apply(&mut model);
        let line = model.trace_line(
            u32::try_from(seq).unwrap_or(u32::MAX),
            event.callback(),
            &event.args(),
            result.as_i32(),
        );
        assert!(line.contains(&format!("|{}|", event.callback())));
        seen.insert(event.callback());
    }
    for callback in CALLBACKS {
        assert!(seen.contains(callback), "missing trace for {callback}");
    }
}

fn random_option(rng: &mut Rng) -> RackOption {
    match rng.below(6) {
        0 => RackOption::PaceMaxSeg,
        1 => RackOption::PaceMinSeg,
        2 => RackOption::PacingDivisor,
        3 => RackOption::PacingBeta,
        4 => RackOption::PacingBetaEcn,
        _ => RackOption::Unknown(999),
    }
}

fn random_segment(rng: &mut Rng) -> SegmentKind {
    match rng.below(4) {
        0 => SegmentKind::Rst,
        1 => SegmentKind::Ack,
        2 => SegmentKind::Data,
        _ => SegmentKind::Fin,
    }
}

fn random_event(rng: &mut Rng) -> Event {
    match rng.below(25) {
        0 => Event::Init,
        1 => Event::Fini(rng.below(2) == 1),
        2..=4 => Event::Output(rng.below(500)),
        5..=6 => Event::Segment(random_segment(rng), rng.below(200)),
        7..=8 => Event::SegmentUnlock(random_segment(rng), rng.below(200)),
        9 => Event::Queued(rng.below(2) == 1),
        10..=11 => Event::Ctl(SockoptDir::Set, random_option(rng), rng.below(2_000)),
        12 => Event::Ctl(SockoptDir::Get, random_option(rng), 0),
        13 => Event::TimerStop,
        14 => Event::Rexmit,
        15 => Event::Handoff,
        16 => Event::Mtu([0, 536, 1_200, 1_460, 9_000][rng.below(5) as usize]),
        17 => Event::Pru(rng.below(4)),
        18 => Event::Hwtls(rng.below(2) == 1),
        19 => Event::Query(Query::from_u32(1 + rng.below(4))),
        20 => Event::Switch,
        21 => Event::Wake,
        22 => Event::Pipe,
        23 => Event::Info,
        _ => Event::Inherit(rng.below(2) == 1),
    }
}

fn run_trace(seed: u64) -> Vec<String> {
    let mut model = VtableModel::new();
    let mut rng = Rng::new(seed);
    let mut trace = Vec::with_capacity(2_000);
    for seq in 0..2_000 {
        let event = random_event(&mut rng);
        let result = event.apply(&mut model);
        trace.push(model.trace_line(
            u32::try_from(seq).unwrap_or(u32::MAX),
            event.callback(),
            &event.args(),
            result.as_i32(),
        ));
        assert!(model.snd_una <= model.snd_max);
        assert_eq!(
            model.hpts_flags & !(PACE_TMR_MASK | PACE_PKT_OUTPUT),
            0,
            "hpts flags"
        );
        if model.fb_ptr {
            assert!(model.compute_pipe() >= 0, "pipe at seq {seq}");
        } else {
            assert_eq!(model.compute_pipe(), -EINVAL);
            assert!(!model.in_hpts);
            assert!(!model.hw_tls);
        }
    }
    trace
}

#[test]
fn ten_thousand_randomized_events_are_deterministic_and_invariant() {
    let seeds = [
        0x3270_0001,
        0x3270_0002,
        0x3270_0003,
        0x3270_0004,
        0x3270_0005,
    ];
    let mut total = 0usize;
    for seed in seeds {
        let first = run_trace(seed);
        let second = run_trace(seed);
        assert_eq!(first, second, "seed {seed:#x} must replay identically");
        assert_eq!(first.len(), 2_000);
        total += first.len();
        for line in &first {
            let fields: Vec<&str> = line.split('|').collect();
            assert_eq!(fields.len(), 17, "trace format: {line}");
        }
    }
    assert_eq!(total, 10_000);
}

#[test]
fn event_result_shapes_are_stable() {
    let mut model = VtableModel::new();
    assert_eq!(Event::Init.apply(&mut model), EventResult::Int(0));
    assert_eq!(
        Event::Ctl(SockoptDir::Get, RackOption::PacingDivisor, 0).apply(&mut model),
        EventResult::Ctl(CtlReply {
            error: 0,
            value: Some(250),
        })
    );
    assert_eq!(Event::Pipe.apply(&mut model), EventResult::Int(0));
    assert_eq!(
        Event::Info.apply(&mut model),
        EventResult::Info(StackInfo {
            error: 0,
            bytes_transmitted: 0,
            bytes_retransmitted: 0,
        })
    );
}
