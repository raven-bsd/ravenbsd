// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Deterministic RTO, keepalive and 2MSL scenarios.

use raven_abi::errno::Errno;
use raven_tcpip::{
    KeepOutcome, RexmtAction, TcpState, TimerState, TwoMslOutcome, tcp_timer_2msl, tcp_timer_keep,
    tcp_timer_rexmt,
};

#[test]
fn rto_retransmits_same_sequence_with_backoff_before_drop() {
    let mut timer = TimerState::new(TcpState::Established, 0);
    timer.snd_una = 1_000;
    timer.snd_nxt = 2_000;
    timer.snd_max = 2_000;
    timer.rexmt_deadline_ms = Some(1_000);

    assert_eq!(
        tcp_timer_rexmt(&mut timer, 999).action,
        RexmtAction::Ignored,
        "the callout has not expired"
    );

    let first = tcp_timer_rexmt(&mut timer, 1_000).action;
    let RexmtAction::Retransmit {
        rxtcur_ms: first_rxtcur,
        syn,
        free_sackholes,
        mtudisc_disabled,
        losing,
    } = first
    else {
        panic!("expected the first retransmit, got {first:?}");
    };
    assert!(!syn);
    assert!(free_sackholes, "the retransmit clears the SACK holes");
    assert!(
        !mtudisc_disabled,
        "no PMTUD fallback on the first retransmit"
    );
    assert!(!losing, "no losing path on the first retransmit");
    assert_eq!(timer.rxtshift, 1);
    assert_eq!(timer.snd_nxt, timer.snd_una, "retransmit starts at snd_una");
    assert_eq!(timer.sackholes, 0, "the retransmit clears the SACK holes");
    assert_eq!(
        timer.snd_fack, timer.snd_una,
        "the retransmit resets snd_fack"
    );
    assert_eq!(
        timer.rexmt_deadline_ms,
        Some(1_000 + first_rxtcur),
        "the callout is rearmed to t_rxtcur"
    );
    assert!(
        (1_000..=4_000).contains(&first_rxtcur),
        "first RTO interval {first_rxtcur} ms is outside the 1..4 s bound"
    );

    let mut retransmits = 1usize;
    let mut previous = first_rxtcur;
    loop {
        let deadline = timer.rexmt_deadline_ms.expect("armed");
        assert_eq!(
            tcp_timer_rexmt(&mut timer, deadline - 1).action,
            RexmtAction::Ignored
        );
        match tcp_timer_rexmt(&mut timer, deadline).action {
            RexmtAction::Retransmit { rxtcur_ms, .. } => {
                retransmits += 1;
                assert!(
                    rxtcur_ms >= previous,
                    "backoff went backwards: {previous} -> {rxtcur_ms}"
                );
                assert_eq!(timer.snd_nxt, timer.snd_una);
                previous = rxtcur_ms;
            }
            RexmtAction::Drop { error } => {
                assert_eq!(error, Errno::ETIMEDOUT);
                assert!(timer.closed);
                break;
            }
            RexmtAction::Ignored => panic!("deadline did not fire"),
        }
        assert!(retransmits <= 16, "the retry limit did not stop the loop");
    }
    assert_eq!(
        retransmits, 12,
        "TCP_MAXRXTSHIFT retransmits before the drop"
    );
}

#[test]
fn keepalive_probes_at_keepidle_and_keepintvl_then_drops_after_maxidle() {
    let mut timer = TimerState::new(TcpState::Established, 0);
    timer.keepalive = true;
    timer.keepidle_ms = 1_000;
    timer.keepintvl_ms = 500;
    timer.keepcnt = 3;
    timer.snd_una = 5_000;
    timer.rcv_nxt = 7_000;
    timer.rcvtime_ms = 0;
    timer.keep_deadline_ms = Some(1_000);

    assert_eq!(
        tcp_timer_keep(&mut timer, 999),
        KeepOutcome::Ignored,
        "the keep callout has not expired"
    );

    let probes = [(1_000, 1_500), (1_500, 2_000), (2_000, 2_500)];
    for (now, next) in probes {
        assert_eq!(
            tcp_timer_keep(&mut timer, now),
            KeepOutcome::Probe {
                seq: 4_999,
                ack: 7_000
            },
            "probe at {now} ms"
        );
        assert_eq!(timer.keep_deadline_ms, Some(next));
    }

    assert_eq!(
        tcp_timer_keep(&mut timer, 2_500),
        KeepOutcome::Drop {
            error: Errno::ETIMEDOUT
        },
        "TP_KEEPIDLE + TP_MAXIDLE expires after three probes"
    );
    assert!(timer.closed);
}

#[test]
fn keepalive_off_rearms_without_a_probe() {
    let mut timer = TimerState::new(TcpState::Established, 0);
    timer.keepalive = false;
    timer.keepidle_ms = 1_000;
    timer.keep_deadline_ms = Some(1_000);
    assert_eq!(tcp_timer_keep(&mut timer, 1_000), KeepOutcome::Rearm);
    assert_eq!(timer.keep_deadline_ms, Some(2_000));
}

#[test]
fn two_msl_closes_time_wait_at_the_armed_deadline() {
    let mut timer = TimerState::new(TcpState::TimeWait, 0);
    timer.arm_two_msl(0, 1_000);
    assert_eq!(timer.two_msl_deadline_ms, Some(2_000), "2 * MSL");
    assert_eq!(tcp_timer_2msl(&mut timer, 1_999), TwoMslOutcome::Ignored);
    assert_eq!(tcp_timer_2msl(&mut timer, 2_000), TwoMslOutcome::Close);
    assert!(timer.closed);

    let mut fin_wait2 = TimerState::new(TcpState::FinWait2, 0);
    fin_wait2.keepintvl_ms = 500;
    fin_wait2.keepcnt = 2;
    fin_wait2.rcvtime_ms = 0;
    // NetBSD `tcp_usrreq.c:193/1209` arms TCPT_2MSL with `t_maxidle` outside TIME_WAIT;
    // the merged `arm_two_msl` helper is the TIME_WAIT 2*MSL arm, so set the donor value here.
    fin_wait2.two_msl_deadline_ms = Some(fin_wait2.maxidle_ms());
    assert_eq!(fin_wait2.two_msl_deadline_ms, Some(1_000), "TP_MAXIDLE");
    assert_eq!(tcp_timer_2msl(&mut fin_wait2, 1_000), TwoMslOutcome::Rearm);
    assert_eq!(fin_wait2.two_msl_deadline_ms, Some(1_500));
    assert_eq!(tcp_timer_2msl(&mut fin_wait2, 1_500), TwoMslOutcome::Close);
    assert!(fin_wait2.closed);
}
