// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the FreeBSD timer port (K267).

use super::*;
use crate::rto::{TCP_BACKOFF, TCP_TOTBACKOFF};
use crate::{Errno, TcpState};

fn state(kind: TcpState) -> TimerState {
    let mut tp = TimerState::new(kind, 0);
    tp.rexmt_deadline_ms = Some(0);
    tp.keep_deadline_ms = Some(0);
    tp.two_msl_deadline_ms = Some(0);
    tp.snd_una = 1_000;
    tp.snd_nxt = 1_000;
    tp.snd_max = 1_000;
    tp.snd_recover = 1_000;
    tp.rcv_nxt = 2_000;
    tp
}

fn fire_rexmt(tp: &mut TimerState, now_ms: u64) -> RexmtOutcome {
    tp.rexmt_deadline_ms = Some(now_ms);
    tcp_timer_rexmt(tp, now_ms)
}

fn retransmit(tp: &mut TimerState, now_ms: u64) -> RexmtAction {
    let outcome = fire_rexmt(tp, now_ms);
    match outcome.action {
        RexmtAction::Retransmit { .. } => outcome.action,
        other => panic!("retransmit expected, got {other:?}"),
    }
}

fn established_keep() -> TimerState {
    let mut tp = state(TcpState::Established);
    tp.keepalive = true;
    tp.keepidle_ms = 7_200_000;
    tp.keepintvl_ms = 75_000;
    tp.keepcnt = 8;
    tp
}

#[test]
fn the_freebsd_backoff_table_and_constants_are_exact() {
    assert_eq!(
        TCP_BACKOFF,
        [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 512, 512, 512]
    );
    assert_eq!(TCP_TOTBACKOFF, 2_559);
    assert_eq!(TCP_MAXRXTSHIFT, 12);
    assert_eq!(TCP_RTT_INVALIDATE, 3);
    assert_eq!(TCPTV_MIN_MS, 30);
    assert_eq!(TCPTV_CPU_VAR_MS, 200);
    assert_eq!(TCPTV_REXMTMAX_MS, 64_000);
    assert_eq!(TCPTV_RTOBASE_MS, 1_000);
    assert_eq!(TCPTV_MSL_MS, 30_000);
    assert_eq!(TCPTV_KEEP_INIT_MS, 75_000);
    assert_eq!(TCPTV_KEEP_IDLE_MS, 7_200_000);
    assert_eq!(TCPTV_KEEPINTVL_MS, 75_000);
    assert_eq!(TCPTV_KEEPCNT, 8);
    assert_eq!(TCPTV_FINWAIT2_TIMEOUT_MS, 60_000);
}

#[test]
fn a_new_timer_state_uses_the_freebsd_defaults() {
    let tp = TimerState::new(TcpState::Established, 123);
    assert_eq!(tp.rcvtime_ms, 123);
    assert_eq!(tp.rttmin_ms, 30);
    assert_eq!(tp.rexmit_slop_ms, 200);
    assert_eq!(tp.rexmit_initial_ms, 1_000);
    assert_eq!(tp.rexmit_max_ms, 64_000);
    assert_eq!(tp.retries, 12);
    assert_eq!(tp.rxtcur_ms, 1_000);
    assert_eq!(tp.keepinit_ms, 75_000);
    assert_eq!(tp.keepidle_ms, 7_200_000);
    assert_eq!(tp.keepintvl_ms, 75_000);
    assert_eq!(tp.keepcnt, 8);
    assert_eq!(tp.maxidle_ms(), 600_000);
    assert_eq!(tp.rtt.rto_ms(), 1_000);
    assert!(!tp.closed);
    assert!(!tp.keepalive);
}

#[test]
fn the_rto_backoff_uses_the_freebsd_table_and_slop() {
    let mut tp = state(TcpState::Established);
    tp.rtt.sample(400);
    assert_eq!(tp.rtt.rexmtval_ms(), 1_200);
    let expected: [(u8, u64); 8] = [
        (1, 2_600),
        (2, 5_000),
        (3, 9_800),
        (4, 19_400),
        (5, 38_600),
        (6, 64_000),
        (7, 64_000),
        (12, 64_000),
    ];
    for (shift, interval) in expected {
        let mut probe = tp.clone();
        probe.rxtshift = shift.saturating_sub(1);
        let action = retransmit(&mut probe, 1_000);
        assert_eq!(probe.rxtcur_ms, interval, "shift {shift}");
        assert_eq!(probe.rexmt_deadline_ms, Some(1_000 + interval));
        let RexmtAction::Retransmit { rxtcur_ms, .. } = action else {
            unreachable!();
        };
        assert_eq!(rxtcur_ms, interval);
    }
}

#[test]
fn the_initial_rto_is_one_second_and_the_first_arm_is_2200ms() {
    let mut tp = state(TcpState::Established);
    let action = retransmit(&mut tp, 1_000);
    assert_eq!(tp.rxtcur_ms, 2_200);
    assert_eq!(tp.rexmt_deadline_ms, Some(3_200));
    let RexmtAction::Retransmit { rxtcur_ms, .. } = action else {
        unreachable!();
    };
    assert_eq!(rxtcur_ms, 2_200);
    let _ = retransmit(&mut tp, 3_200);
    assert_eq!(tp.rxtcur_ms, 4_200);
}

#[test]
fn a_syn_retransmit_uses_the_initial_rto_not_the_sample() {
    let mut tp = state(TcpState::SynSent);
    tp.rtt.sample(400);
    assert_eq!(tp.rtt.rexmtval_ms(), 1_200);
    let action = retransmit(&mut tp, 1_000);
    let RexmtAction::Retransmit { syn, rxtcur_ms, .. } = action else {
        unreachable!();
    };
    assert!(syn);
    assert_eq!(rxtcur_ms, 2_200);
}

#[test]
fn a_connection_drops_after_thirteen_retransmit_timeouts() {
    let mut tp = state(TcpState::Established);
    for shift in 1..=TCP_MAXRXTSHIFT {
        let _ = retransmit(&mut tp, u64::from(shift) * 1_000);
        assert_eq!(tp.rxtshift, shift);
    }
    let outcome = fire_rexmt(&mut tp, 100_000);
    assert_eq!(
        outcome.action,
        RexmtAction::Drop {
            error: Errno::ETIMEDOUT
        }
    );
    assert_eq!(tp.rxtshift, TCP_MAXRXTSHIFT);
    assert!(tp.closed);
    assert_eq!(tp.rexmt_deadline_ms, None);
}

#[test]
fn maxunacktime_drops_a_connection_without_progress() {
    let mut tp = state(TcpState::Established);
    tp.maxunacktime_ms = 1_000;
    tp.acktime_ms = 100;
    assert!(!tp.maxunacktime_exceeded(1_099));
    assert!(tp.maxunacktime_exceeded(1_100));
    let outcome = fire_rexmt(&mut tp, 1_200);
    assert_eq!(
        outcome.action,
        RexmtAction::Drop {
            error: Errno::ETIMEDOUT
        }
    );
}

#[test]
fn the_first_retransmit_keeps_sackholes_for_resend_and_later_ones_free_them() {
    let mut tp = state(TcpState::Established);
    tp.sackholes = 3;
    let action = retransmit(&mut tp, 1_000);
    let RexmtAction::Retransmit {
        first,
        resend_sackholes,
        ..
    } = action
    else {
        unreachable!();
    };
    assert!(first);
    assert!(resend_sackholes);
    assert_eq!(tp.sackholes, 3);

    let action = retransmit(&mut tp, 3_200);
    let RexmtAction::Retransmit {
        first,
        resend_sackholes,
        ..
    } = action
    else {
        unreachable!();
    };
    assert!(!first);
    assert!(!resend_sackholes);
    assert_eq!(tp.sackholes, 0);
}

#[test]
fn rexmt_rewinds_the_send_pointer_and_forces_an_ack() {
    let mut tp = state(TcpState::Established);
    tp.snd_una = 5_000;
    tp.snd_nxt = 5_005;
    tp.snd_max = 6_000;
    tp.snd_recover = 5_005;
    tp.rtttime_active = true;
    tp.acknow = false;
    let action = retransmit(&mut tp, 1_000);
    let RexmtAction::Retransmit { syn, .. } = action else {
        unreachable!();
    };
    assert!(!syn);
    assert_eq!(tp.snd_nxt, 5_000);
    assert_eq!(tp.snd_recover, 6_000);
    assert!(tp.acknow);
    assert!(!tp.rtttime_active);
}

#[test]
fn a_lost_fin_is_retransmitted_from_snd_una() {
    let mut tp = state(TcpState::FinWait1);
    tp.snd_una = 5_000;
    tp.snd_nxt = 5_001;
    tp.snd_max = 5_001;
    let _ = retransmit(&mut tp, 1_000);
    assert_eq!(tp.snd_nxt, tp.snd_una);
}

#[test]
fn rtt_is_invalidated_after_the_fourth_retransmit() {
    let mut tp = state(TcpState::Established);
    tp.rtt.sample(400);
    for shift in 1u8..=3 {
        let action = retransmit(&mut tp, u64::from(shift) * 1_000);
        let RexmtAction::Retransmit {
            rtt_invalidated, ..
        } = action
        else {
            unreachable!();
        };
        assert!(!rtt_invalidated, "shift {shift}");
    }
    let action = retransmit(&mut tp, 4_000);
    let RexmtAction::Retransmit {
        rtt_invalidated, ..
    } = action
    else {
        unreachable!();
    };
    assert!(rtt_invalidated);
    tp.rtt.sample(200);
    assert_eq!(tp.rtt.srtt_ms(), 200);
    assert_eq!(tp.rtt.rttvar_ms(), 100);
}

#[test]
fn plpmtud_blackhole_detection_starts_on_the_first_retransmit() {
    for kind in [TcpState::Established, TcpState::FinWait1] {
        let mut tp = state(kind);
        let action = retransmit(&mut tp, 1_000);
        let RexmtAction::Retransmit {
            blackhole_detect, ..
        } = action
        else {
            unreachable!();
        };
        assert!(blackhole_detect, "{kind:?}");
    }
    for kind in [
        TcpState::SynSent,
        TcpState::SynReceived,
        TcpState::CloseWait,
    ] {
        let mut tp = state(kind);
        let action = retransmit(&mut tp, 1_000);
        let RexmtAction::Retransmit {
            blackhole_detect, ..
        } = action
        else {
            unreachable!();
        };
        assert!(!blackhole_detect, "{kind:?}");
    }
}

#[test]
fn drop_options_fires_on_the_third_syn_retransmit() {
    let mut tp = state(TcpState::SynSent);
    tp.rexmit_drop_options = true;
    for shift in 1u8..=2 {
        let action = retransmit(&mut tp, u64::from(shift) * 1_000);
        let RexmtAction::Retransmit { drop_options, .. } = action else {
            unreachable!();
        };
        assert!(!drop_options, "shift {shift}");
    }
    let action = retransmit(&mut tp, 3_000);
    let RexmtAction::Retransmit { drop_options, .. } = action else {
        unreachable!();
    };
    assert!(drop_options);
}

#[test]
fn rexmt_ignores_a_closed_or_unexpired_callout() {
    let mut closed = state(TcpState::Established);
    closed.closed = true;
    assert_eq!(
        tcp_timer_rexmt(&mut closed, 1_000).action,
        RexmtAction::Ignored
    );

    let mut waiting = state(TcpState::Established);
    waiting.rexmt_deadline_ms = Some(2_000);
    assert_eq!(
        tcp_timer_rexmt(&mut waiting, 1_999).action,
        RexmtAction::Ignored
    );
    assert_eq!(waiting.rxtshift, 0);
}

#[test]
fn keep_early_rearms_when_idle_is_below_keepidle() {
    let mut tp = established_keep();
    let outcome = tcp_timer_keep(&mut tp, 1_000);
    assert_eq!(outcome, KeepOutcome::Rearm);
    assert_eq!(tp.keep_deadline_ms, Some(7_200_000));
}

#[test]
fn keepalive_probes_with_snd_una_minus_one() {
    let mut tp = established_keep();
    let outcome = tcp_timer_keep(&mut tp, 7_200_000);
    assert_eq!(
        outcome,
        KeepOutcome::Probe {
            seq: 999,
            ack: 2_000
        }
    );
    assert_eq!(tp.keep_deadline_ms, Some(7_275_000));
}

#[test]
fn keepalive_drops_at_keepidle_plus_maxidle() {
    let mut tp = established_keep();
    assert!(matches!(
        tcp_timer_keep(&mut tp, 7_799_999),
        KeepOutcome::Probe { .. }
    ));

    let mut tp = established_keep();
    let outcome = tcp_timer_keep(&mut tp, 7_800_000);
    assert_eq!(
        outcome,
        KeepOutcome::Drop {
            error: Errno::ETIMEDOUT
        }
    );
    assert!(tp.closed);
    assert_eq!(tp.keep_deadline_ms, None);
}

#[test]
fn keep_drops_when_the_state_is_not_established() {
    for kind in [
        TcpState::Closed,
        TcpState::Listen,
        TcpState::SynSent,
        TcpState::SynReceived,
    ] {
        let mut tp = state(kind);
        assert_eq!(
            tcp_timer_keep(&mut tp, 1_000),
            KeepOutcome::Drop {
                error: Errno::ETIMEDOUT
            },
            "{kind:?}"
        );
    }
}

#[test]
fn keep_probes_through_closing_and_rearms_past_it() {
    for kind in [
        TcpState::Established,
        TcpState::FinWait1,
        TcpState::FinWait2,
        TcpState::CloseWait,
        TcpState::Closing,
    ] {
        let mut tp = established_keep();
        tp.state = kind;
        assert!(
            matches!(
                tcp_timer_keep(&mut tp, 7_200_000),
                KeepOutcome::Probe { .. }
            ),
            "{kind:?}"
        );
    }
    for kind in [TcpState::LastAck, TcpState::TimeWait] {
        let mut tp = established_keep();
        tp.state = kind;
        assert_eq!(
            tcp_timer_keep(&mut tp, 7_200_000),
            KeepOutcome::Rearm,
            "{kind:?}"
        );
    }
}

#[test]
fn always_keepalive_probes_without_the_socket_option() {
    let mut tp = established_keep();
    tp.keepalive = false;
    tp.always_keepalive = true;
    assert!(matches!(
        tcp_timer_keep(&mut tp, 7_200_000),
        KeepOutcome::Probe { .. }
    ));

    let mut tp = established_keep();
    tp.keepalive = false;
    assert_eq!(tcp_timer_keep(&mut tp, 7_200_000), KeepOutcome::Rearm);
}

#[test]
fn keep_ignores_a_closed_or_unexpired_callout() {
    let mut closed = established_keep();
    closed.closed = true;
    assert_eq!(tcp_timer_keep(&mut closed, 7_200_000), KeepOutcome::Ignored);

    let mut waiting = established_keep();
    waiting.keep_deadline_ms = Some(7_300_000);
    assert_eq!(
        tcp_timer_keep(&mut waiting, 7_299_999),
        KeepOutcome::Ignored
    );
}

#[test]
fn two_msl_closes_time_wait_and_frees_sackholes() {
    let mut tp = state(TcpState::TimeWait);
    tp.sackholes = 3;
    assert_eq!(tcp_timer_2msl(&mut tp, 60_000), TwoMslOutcome::Close);
    assert!(tp.closed);
    assert_eq!(tp.two_msl_deadline_ms, None);
    assert_eq!(tp.sackholes, 0);
}

#[test]
fn two_msl_fast_recycles_fin_wait_two_when_the_receiver_closed() {
    let mut tp = state(TcpState::FinWait2);
    tp.fast_finwait2_recycle = true;
    tp.receiver_closed = true;
    assert_eq!(tcp_timer_2msl(&mut tp, 1_000), TwoMslOutcome::Close);
    assert!(tp.closed);
}

#[test]
fn two_msl_rearms_in_fin_wait_two_inside_maxidle() {
    let mut tp = state(TcpState::FinWait2);
    tp.keepintvl_ms = 75_000;
    tp.keepcnt = 8;
    assert_eq!(tcp_timer_2msl(&mut tp, 600_000), TwoMslOutcome::Rearm);
    assert_eq!(tp.two_msl_deadline_ms, Some(675_000));
    assert!(!tp.closed);
}

#[test]
fn two_msl_closes_an_idle_fin_wait_two() {
    let mut tp = state(TcpState::FinWait2);
    tp.keepcnt = 8;
    tp.keepintvl_ms = 75_000;
    assert_eq!(tcp_timer_2msl(&mut tp, 600_001), TwoMslOutcome::Close);
    assert!(tp.closed);
}

#[test]
fn two_msl_ignores_a_closed_or_unexpired_callout() {
    let mut closed = state(TcpState::TimeWait);
    closed.closed = true;
    assert_eq!(tcp_timer_2msl(&mut closed, 60_000), TwoMslOutcome::Ignored);

    let mut waiting = state(TcpState::TimeWait);
    waiting.two_msl_deadline_ms = Some(60_000);
    assert_eq!(tcp_timer_2msl(&mut waiting, 59_999), TwoMslOutcome::Ignored);
}

#[test]
fn arm_two_msl_uses_two_msl_for_time_wait_and_maxidle_for_fin_wait_two() {
    let mut tp = TimerState::new(TcpState::TimeWait, 0);
    tp.arm_two_msl(123, 30_000);
    assert_eq!(tp.two_msl_deadline_ms, Some(60_123));

    let mut tp = TimerState::new(TcpState::FinWait2, 0);
    tp.keepcnt = 8;
    tp.keepintvl_ms = 75_000;
    tp.arm_two_msl(123, 30_000);
    assert_eq!(tp.two_msl_deadline_ms, Some(600_123));

    let mut tp = TimerState::new(TcpState::FinWait2, 0);
    tp.fast_finwait2_recycle = true;
    tp.arm_two_msl(123, 30_000);
    assert_eq!(tp.two_msl_deadline_ms, Some(60_123));
}

#[test]
fn zero_keepalive_options_use_the_global_defaults() {
    let mut tp = TimerState::new(TcpState::Established, 0);
    tp.keepinit_ms = 0;
    tp.keepidle_ms = 0;
    tp.keepintvl_ms = 0;
    tp.keepcnt = 0;
    assert_eq!(tp.effective_keepinit_ms(), 75_000);
    assert_eq!(tp.effective_keepidle_ms(), 7_200_000);
    assert_eq!(tp.effective_keepintvl_ms(), 75_000);
    assert_eq!(tp.maxidle_ms(), 600_000);
}

#[test]
fn arm_keep_init_uses_the_connect_interval() {
    let mut tp = TimerState::new(TcpState::SynSent, 0);
    tp.arm_keep_init(500);
    assert_eq!(tp.keep_deadline_ms, Some(75_500));
}

#[test]
fn the_state_predicates_match_the_freebsd_macros() {
    for kind in [
        TcpState::Established,
        TcpState::FinWait1,
        TcpState::FinWait2,
        TcpState::CloseWait,
        TcpState::Closing,
        TcpState::LastAck,
        TcpState::TimeWait,
    ] {
        assert!(established(kind), "{kind:?}");
    }
    for kind in [
        TcpState::Closed,
        TcpState::Listen,
        TcpState::SynSent,
        TcpState::SynReceived,
    ] {
        assert!(!established(kind), "{kind:?}");
    }
    for kind in [
        TcpState::Established,
        TcpState::FinWait1,
        TcpState::FinWait2,
        TcpState::CloseWait,
        TcpState::Closing,
    ] {
        assert!(keep_state_allows_probe(kind), "{kind:?}");
    }
    for kind in [
        TcpState::Closed,
        TcpState::Listen,
        TcpState::SynSent,
        TcpState::SynReceived,
        TcpState::LastAck,
        TcpState::TimeWait,
    ] {
        assert!(!keep_state_allows_probe(kind), "{kind:?}");
    }
}

#[test]
fn callout_expiry_is_inclusive() {
    assert!(callout_expired(Some(100), 100));
    assert!(callout_expired(Some(100), 101));
    assert!(!callout_expired(Some(100), 99));
    assert!(!callout_expired(None, 1_000));
}
