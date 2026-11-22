// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! NetBSD `tcp_timer.c` behavior tests.
//!
//! The deterministic cases match the ported NetBSD timer logic: the backoff table/range, SACK
//! scoreboard clear, PMTUD black-hole shift, losing reset, keepalive probes/drop and 2MSL close.
//! The FreeBSD-only cases (maxunacktime, always-keepalive, fast FIN_WAIT_2 recycle, SYN options
//! drop, first-retransmit SACK resend) are replaced by the NetBSD behavior in this file.

use super::*;
use crate::rto::{TCP_BACKOFF, TCP_TOTBACKOFF};

fn timer(state: TcpState) -> TimerState {
    TimerState::new(state, 0)
}

fn rexmt_timer(state: TcpState) -> TimerState {
    let mut tp = timer(state);
    tp.snd_una = 100;
    tp.snd_max = 300;
    tp.snd_high = 0;
    tp.rexmt_deadline_ms = Some(1_000);
    tp
}

fn keep_timer() -> TimerState {
    let mut tp = timer(TcpState::Established);
    tp.keep_deadline_ms = Some(1_000);
    tp.keepalive = true;
    tp.snd_una = 41;
    tp.rcv_nxt = 77;
    tp.keepidle_ms = 75_000;
    tp.keepintvl_ms = 75_000;
    tp.maxidle_ms = 600_000;
    tp
}

#[test]
fn netbsd_backoff_table_and_constants_are_exact() {
    assert_eq!(
        TCP_BACKOFF,
        [1, 2, 4, 8, 16, 32, 64, 64, 64, 64, 64, 64, 64]
    );
    assert_eq!(TCP_TOTBACKOFF, 511);
    assert_eq!(TCP_MAXRXTSHIFT, 12);
    assert_eq!(TCPTV_MIN_MS, 500);
    assert_eq!(TCPTV_REXMTMAX_MS, 32_000);
    assert_eq!(TCPTV_MSL_MS, 30_000);
    assert_eq!(TCPTV_KEEP_INIT_MS, 75_000);
    assert_eq!(TCPTV_KEEP_IDLE_MS, 7_200_000);
    assert_eq!(TCPTV_KEEPINTVL_MS, 75_000);
    assert_eq!(TCPTV_KEEPCNT, 8);
    assert_eq!(TCPTV_MAXIDLE_MS, 600_000);
}

#[test]
fn a_new_timer_state_uses_the_netbsd_defaults() {
    let tp = timer(TcpState::Established);
    assert!(!tp.closed);
    assert!(!tp.keepalive);
    assert_eq!(tp.rttmin_ms, TCPTV_MIN_MS);
    assert_eq!(tp.rexmit_max_ms, TCPTV_REXMTMAX_MS);
    assert_eq!(tp.retries, TCP_MAXRXTSHIFT);
    assert_eq!(tp.rxtshift, 0);
    assert_eq!(tp.rxtcur_ms, TCPTV_RTOBASE_MS);
    assert_eq!(tp.keepinit_ms, TCPTV_KEEP_INIT_MS);
    assert_eq!(tp.keepidle_ms, TCPTV_KEEP_IDLE_MS);
    assert_eq!(tp.keepintvl_ms, TCPTV_KEEPINTVL_MS);
    assert_eq!(tp.keepcnt, TCPTV_KEEPCNT);
    assert_eq!(tp.maxidle_ms, TCPTV_MAXIDLE_MS);
    assert!(tp.mtudisc);
    assert!(!tp.syn_rexmt);
    assert_eq!(tp.rexmt_deadline_ms, None);
}

#[test]
fn rexmt_ignores_a_closed_or_unexpired_callout() {
    let mut closed = rexmt_timer(TcpState::Established);
    closed.closed = true;
    assert_eq!(
        tcp_timer_rexmt(&mut closed, 1_000).action,
        RexmtAction::Ignored
    );

    let mut unexpired = rexmt_timer(TcpState::Established);
    assert_eq!(
        tcp_timer_rexmt(&mut unexpired, 999).action,
        RexmtAction::Ignored
    );
}

#[test]
fn the_first_retransmit_uses_rttmin_times_backoff_one() {
    let mut tp = rexmt_timer(TcpState::Established);
    let RexmtAction::Retransmit {
        rxtcur_ms,
        syn,
        pmtud_blackhole,
        losing_reset,
    } = tcp_timer_rexmt(&mut tp, 1_000).action
    else {
        panic!("expected retransmit");
    };
    // No sample: TCP_REXMTVAL is 0, clamped to tcp_rttmin (500 ms), times tcp_backoff[1] = 2.
    assert_eq!(rxtcur_ms, 1_000);
    assert!(!syn);
    assert!(!pmtud_blackhole);
    assert!(!losing_reset);
    assert_eq!(tp.rxtshift, 1);
    assert_eq!(tp.rexmt_deadline_ms, Some(2_000));
    assert_eq!(tp.snd_nxt, 100);
    assert_eq!(tp.snd_high, 300);
    assert!(!tp.rtttime_active);
}

#[test]
fn the_backoff_doubles_and_caps_at_64_ticks() {
    let mut tp = rexmt_timer(TcpState::Established);
    let expected = [1_000, 2_000, 4_000, 8_000, 16_000, 32_000, 32_000, 32_000];
    for (index, expected_ms) in expected.iter().enumerate() {
        let now = u64::try_from(index).unwrap() * 32_000 + 1_000;
        tp.rexmt_deadline_ms = Some(now);
        let RexmtAction::Retransmit { rxtcur_ms, .. } = tcp_timer_rexmt(&mut tp, now).action else {
            panic!("expected retransmit at shift {index}");
        };
        assert_eq!(rxtcur_ms, *expected_ms, "shift {index}");
        assert_eq!(tp.rxtshift as usize, index + 1);
    }
}

#[test]
fn a_connection_drops_after_thirteen_retransmit_timeouts() {
    let mut tp = rexmt_timer(TcpState::Established);
    for shift in 1..=TCP_MAXRXTSHIFT {
        tp.rexmt_deadline_ms = Some(u64::from(shift));
        let outcome = tcp_timer_rexmt(&mut tp, u64::from(shift));
        assert!(matches!(outcome.action, RexmtAction::Retransmit { .. }));
    }
    tp.rexmt_deadline_ms = Some(99);
    let outcome = tcp_timer_rexmt(&mut tp, 99);
    assert_eq!(
        outcome.action,
        RexmtAction::Drop {
            error: Errno::ETIMEDOUT
        }
    );
    assert!(tp.closed);
    assert_eq!(tp.rexmt_deadline_ms, None);
}

#[test]
fn a_syn_retransmit_arms_one_second_and_sets_syn_rexmt() {
    let mut tp = rexmt_timer(TcpState::SynSent);
    let RexmtAction::Retransmit { rxtcur_ms, syn, .. } = tcp_timer_rexmt(&mut tp, 1_000).action
    else {
        panic!("expected retransmit");
    };
    assert_eq!(rxtcur_ms, 1_000);
    assert!(syn);
    assert!(tp.syn_rexmt);
}

#[test]
fn rexmt_clears_the_sack_scoreboard_and_resets_snd_fack() {
    let mut tp = rexmt_timer(TcpState::Established);
    tp.sackholes = 4;
    tp.snd_fack = 250;
    let _ = tcp_timer_rexmt(&mut tp, 1_000);
    assert_eq!(tp.sackholes, 0);
    assert_eq!(tp.snd_fack, 100);
}

#[test]
fn a_sample_sets_the_tcp_rexmtval_base() {
    let mut tp = rexmt_timer(TcpState::Established);
    tp.rtt.sample(400);
    assert_eq!(tp.rtt.rexmtval_ms(), 1_200);
    let RexmtAction::Retransmit { rxtcur_ms, .. } = tcp_timer_rexmt(&mut tp, 1_000).action else {
        panic!("expected retransmit");
    };
    // max(1200, 500) * backoff[1] = 2400 ms.
    assert_eq!(rxtcur_ms, 2_400);
}

#[test]
fn pmtud_black_hole_turns_off_after_the_second_retransmit() {
    let mut tp = rexmt_timer(TcpState::Established);
    for shift in 1..=PMTUD_BLACKHOLE_SHIFT {
        tp.rexmt_deadline_ms = Some(u64::from(shift));
        let RexmtAction::Retransmit {
            pmtud_blackhole, ..
        } = tcp_timer_rexmt(&mut tp, u64::from(shift)).action
        else {
            panic!("expected retransmit");
        };
        assert!(!pmtud_blackhole, "shift {shift}");
        assert!(tp.mtudisc);
    }
    tp.rexmt_deadline_ms = Some(3);
    let RexmtAction::Retransmit {
        pmtud_blackhole, ..
    } = tcp_timer_rexmt(&mut tp, 3).action
    else {
        panic!("expected retransmit");
    };
    assert!(pmtud_blackhole);
    assert!(!tp.mtudisc);
}

#[test]
fn the_losing_reset_moves_srtt_into_rttvar() {
    let mut tp = rexmt_timer(TcpState::Established);
    tp.rtt.sample(400);
    let srtt_before = tp.rtt.srtt_q8();
    let rttvar_before = tp.rtt.rttvar_q8();
    for shift in 1..=TCP_RTT_INVALIDATE {
        tp.rexmt_deadline_ms = Some(u64::from(shift));
        let RexmtAction::Retransmit { losing_reset, .. } =
            tcp_timer_rexmt(&mut tp, u64::from(shift)).action
        else {
            panic!("expected retransmit");
        };
        assert!(!losing_reset, "shift {shift}");
    }
    tp.rexmt_deadline_ms = Some(4);
    let RexmtAction::Retransmit { losing_reset, .. } = tcp_timer_rexmt(&mut tp, 4).action else {
        panic!("expected retransmit");
    };
    assert!(losing_reset);
    assert_eq!(tp.rtt.srtt_q8(), 0);
    assert_eq!(tp.rtt.rttvar_q8(), rttvar_before + srtt_before / 4);
}

#[test]
fn keep_ignores_a_closed_or_unexpired_callout() {
    let mut closed = keep_timer();
    closed.closed = true;
    assert_eq!(tcp_timer_keep(&mut closed, 1_000), KeepOutcome::Ignored);

    let mut unexpired = keep_timer();
    assert_eq!(tcp_timer_keep(&mut unexpired, 999), KeepOutcome::Ignored);
}

#[test]
fn keep_drops_an_unestablished_connection() {
    let mut tp = timer(TcpState::SynSent);
    tp.keep_deadline_ms = Some(1_000);
    assert_eq!(
        tcp_timer_keep(&mut tp, 1_000),
        KeepOutcome::Drop {
            error: Errno::ETIMEDOUT
        }
    );
    assert!(tp.closed);
}

#[test]
fn keep_probes_with_snd_una_minus_one() {
    let mut tp = keep_timer();
    assert_eq!(
        tcp_timer_keep(&mut tp, 1_000),
        KeepOutcome::Probe { seq: 40, ack: 77 }
    );
    assert_eq!(tp.keep_deadline_ms, Some(76_000));
}

#[test]
fn keep_drops_at_keepidle_plus_maxidle() {
    let mut tp = keep_timer();
    tp.keepidle_ms = 500;
    tp.maxidle_ms = 1_000;
    tp.rcvtime_ms = 0;
    assert_eq!(
        tcp_timer_keep(&mut tp, 1_500),
        KeepOutcome::Drop {
            error: Errno::ETIMEDOUT
        }
    );
    assert!(tp.closed);
}

#[test]
fn keep_without_the_socket_option_rearms_at_keepidle() {
    let mut tp = keep_timer();
    tp.keepalive = false;
    assert_eq!(tcp_timer_keep(&mut tp, 1_000), KeepOutcome::Rearm);
    assert_eq!(tp.keep_deadline_ms, Some(76_000));
}

#[test]
fn keep_rearms_in_states_past_close_wait() {
    for state in [TcpState::Closing, TcpState::LastAck, TcpState::TimeWait] {
        let mut tp = keep_timer();
        tp.state = state;
        assert_eq!(
            tcp_timer_keep(&mut tp, 1_000),
            KeepOutcome::Rearm,
            "{state:?}"
        );
        assert_eq!(tp.keep_deadline_ms, Some(76_000), "{state:?}");
    }
}

#[test]
fn two_msl_closes_time_wait_and_clears_the_scoreboard() {
    let mut tp = timer(TcpState::TimeWait);
    tp.two_msl_deadline_ms = Some(1_000);
    tp.sackholes = 3;
    tp.snd_fack = 9;
    tp.snd_una = 12;
    assert_eq!(tcp_timer_2msl(&mut tp, 1_000), TwoMslOutcome::Close);
    assert_eq!(tp.sackholes, 0);
    assert_eq!(tp.snd_fack, 12);
    assert!(tp.closed);
}

#[test]
fn two_msl_rearms_inside_maxidle() {
    let mut tp = timer(TcpState::FinWait2);
    tp.two_msl_deadline_ms = Some(1_000);
    tp.maxidle_ms = 600_000;
    tp.rcvtime_ms = 0;
    tp.keepintvl_ms = 75_000;
    assert_eq!(tcp_timer_2msl(&mut tp, 1_000), TwoMslOutcome::Rearm);
    assert_eq!(tp.two_msl_deadline_ms, Some(76_000));
}

#[test]
fn two_msl_closes_an_idle_fin_wait_two() {
    let mut tp = timer(TcpState::FinWait2);
    tp.two_msl_deadline_ms = Some(1_000);
    tp.maxidle_ms = 500;
    tp.rcvtime_ms = 0;
    assert_eq!(tcp_timer_2msl(&mut tp, 1_000), TwoMslOutcome::Close);
    assert!(tp.closed);
}

#[test]
fn two_msl_rearms_when_maxidle_is_zero() {
    let mut tp = timer(TcpState::FinWait2);
    tp.two_msl_deadline_ms = Some(1_000);
    tp.maxidle_ms = 0;
    tp.keepintvl_ms = 75_000;
    assert_eq!(tcp_timer_2msl(&mut tp, 1_000), TwoMslOutcome::Rearm);
    assert_eq!(tp.two_msl_deadline_ms, Some(76_000));
}

#[test]
fn arm_helpers_use_the_netbsd_intervals() {
    let mut tp = timer(TcpState::SynSent);
    tp.arm_keep_init(10);
    assert_eq!(tp.keep_deadline_ms, Some(75_010));
    tp.arm_two_msl(10, TCPTV_MSL_MS);
    assert_eq!(tp.two_msl_deadline_ms, Some(60_010));
}

#[test]
fn state_predicates_match_the_netbsd_macros() {
    let established_states = [
        TcpState::Established,
        TcpState::FinWait1,
        TcpState::FinWait2,
        TcpState::CloseWait,
        TcpState::Closing,
        TcpState::LastAck,
        TcpState::TimeWait,
    ];
    for state in established_states {
        assert!(established(state), "{state:?}");
    }
    for state in [
        TcpState::Closed,
        TcpState::Listen,
        TcpState::SynSent,
        TcpState::SynReceived,
    ] {
        assert!(!established(state), "{state:?}");
    }
    for state in [
        TcpState::Established,
        TcpState::FinWait1,
        TcpState::FinWait2,
        TcpState::CloseWait,
    ] {
        assert!(keep_probe_allowed(state), "{state:?}");
    }
    for state in [
        TcpState::Closed,
        TcpState::Listen,
        TcpState::SynSent,
        TcpState::SynReceived,
        TcpState::Closing,
        TcpState::LastAck,
        TcpState::TimeWait,
    ] {
        assert!(!keep_probe_allowed(state), "{state:?}");
    }
}

#[test]
fn callout_expiry_is_inclusive() {
    assert!(!callout_expired(None, 1_000));
    assert!(!callout_expired(Some(1_001), 1_000));
    assert!(callout_expired(Some(1_000), 1_000));
    assert!(callout_expired(Some(999), 1_000));
}

#[test]
fn rexmt_ignored_keeps_the_deadline_and_shift() {
    let mut tp = rexmt_timer(TcpState::Established);
    assert_eq!(tcp_timer_rexmt(&mut tp, 999).action, RexmtAction::Ignored);
    assert_eq!(tp.rxtshift, 0);
    assert_eq!(tp.rexmt_deadline_ms, Some(1_000));
}

#[test]
fn rexmt_sample_clamps_at_rexmit_max() {
    let mut tp = rexmt_timer(TcpState::Established);
    tp.rtt.sample(1_000);
    assert_eq!(tp.rtt.rexmtval_ms(), 3_000);
    tp.rxtshift = 5;
    let RexmtAction::Retransmit { rxtcur_ms, .. } = tcp_timer_rexmt(&mut tp, 1_000).action else {
        panic!("expected retransmit");
    };
    assert_eq!(rxtcur_ms, TCPTV_REXMTMAX_MS);
}

#[test]
fn rexmt_drop_clears_all_callouts() {
    let mut tp = rexmt_timer(TcpState::Established);
    tp.rxtshift = TCP_MAXRXTSHIFT;
    tp.keep_deadline_ms = Some(1);
    tp.two_msl_deadline_ms = Some(1);
    let outcome = tcp_timer_rexmt(&mut tp, 1_000);
    assert_eq!(
        outcome.action,
        RexmtAction::Drop {
            error: Errno::ETIMEDOUT
        }
    );
    assert_eq!(tp.rexmt_deadline_ms, None);
    assert_eq!(tp.keep_deadline_ms, None);
    assert_eq!(tp.two_msl_deadline_ms, None);
}

#[test]
fn keep_with_zero_maxidle_never_drops() {
    let mut tp = keep_timer();
    tp.maxidle_ms = 0;
    tp.rcvtime_ms = 0;
    assert_eq!(
        tcp_timer_keep(&mut tp, 1_000_000),
        KeepOutcome::Probe { seq: 40, ack: 77 }
    );
}

#[test]
fn keep_drop_clears_all_callouts() {
    let mut tp = keep_timer();
    tp.keepidle_ms = 500;
    tp.maxidle_ms = 500;
    tp.rcvtime_ms = 0;
    tp.rexmt_deadline_ms = Some(1);
    tp.two_msl_deadline_ms = Some(1);
    assert_eq!(
        tcp_timer_keep(&mut tp, 1_000),
        KeepOutcome::Drop {
            error: Errno::ETIMEDOUT
        }
    );
    assert_eq!(tp.rexmt_deadline_ms, None);
    assert_eq!(tp.keep_deadline_ms, None);
    assert_eq!(tp.two_msl_deadline_ms, None);
}

#[test]
fn keep_probes_in_close_wait() {
    let mut tp = keep_timer();
    tp.state = TcpState::CloseWait;
    assert_eq!(
        tcp_timer_keep(&mut tp, 1_000),
        KeepOutcome::Probe { seq: 40, ack: 77 }
    );
}

#[test]
fn two_msl_rearm_keeps_the_snd_fack_reset() {
    let mut tp = timer(TcpState::FinWait2);
    tp.two_msl_deadline_ms = Some(1_000);
    tp.maxidle_ms = 600_000;
    tp.rcvtime_ms = 0;
    tp.keepintvl_ms = 75_000;
    tp.sackholes = 2;
    tp.snd_fack = 99;
    tp.snd_una = 5;
    assert_eq!(tcp_timer_2msl(&mut tp, 1_000), TwoMslOutcome::Rearm);
    assert_eq!(tp.sackholes, 0);
    assert_eq!(tp.snd_fack, 5);
    assert_eq!(tp.two_msl_deadline_ms, Some(76_000));
}

#[test]
fn two_msl_close_clears_all_callouts() {
    let mut tp = timer(TcpState::TimeWait);
    tp.two_msl_deadline_ms = Some(1_000);
    tp.rexmt_deadline_ms = Some(1);
    tp.keep_deadline_ms = Some(1);
    assert_eq!(tcp_timer_2msl(&mut tp, 1_000), TwoMslOutcome::Close);
    assert_eq!(tp.rexmt_deadline_ms, None);
    assert_eq!(tp.keep_deadline_ms, None);
    assert_eq!(tp.two_msl_deadline_ms, None);
}

#[test]
fn arm_two_msl_accepts_zero_msl() {
    let mut tp = timer(TcpState::TimeWait);
    tp.arm_two_msl(10, 0);
    assert_eq!(tp.two_msl_deadline_ms, Some(10));
}

#[test]
fn arm_keep_init_accepts_zero_keepinit() {
    let mut tp = timer(TcpState::SynSent);
    tp.keepinit_ms = 0;
    tp.arm_keep_init(10);
    assert_eq!(tp.keep_deadline_ms, Some(10));
}

#[test]
fn losing_reset_with_zero_srtt_is_a_noop() {
    let mut estimator = RtoEstimator::new(1_000, 30, 64_000, 1);
    estimator.losing_reset();
    assert_eq!(estimator.srtt_q8(), 0);
    assert_eq!(estimator.rttvar_q8(), 0);
    assert_eq!(estimator.rexmtval_ms(), 1_000);
}

#[test]
fn backoff_clamps_to_the_minimum_and_maximum() {
    assert_eq!(RtoEstimator::backoff(1, 1, 500, 32_000), 1_000);
    assert_eq!(RtoEstimator::backoff(100_000, 12, 500, 32_000), 32_000);
}
