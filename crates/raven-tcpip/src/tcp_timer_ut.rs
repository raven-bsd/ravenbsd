// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the NetBSD timer port (K256).
//!
//! The deterministic rows pin the exact `sys/netinet/tcp_timer.c` equations; the randomized
//! model below is an independent integer implementation of the same NetBSD equations and drives
//! the product `TimerState`/entry points, so a change in either side is caught.

use super::*;
use crate::rto::{TCP_BACKOFF, TCP_TOTBACKOFF};
use crate::{Errno, TcpState};
use alloc::format;

fn state(kind: TcpState) -> TimerState {
    let mut tp = TimerState::new(kind, 0);
    tp.rexmt_deadline_ms = Some(0);
    tp.keep_deadline_ms = Some(0);
    tp.two_msl_deadline_ms = Some(0);
    tp.snd_una = 1_000;
    tp.snd_nxt = 1_000;
    tp.snd_max = 1_000;
    tp.snd_high = 1_000;
    tp.snd_fack = 1_000;
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
fn the_netbsd_backoff_table_and_constants_are_exact() {
    assert_eq!(
        TCP_BACKOFF,
        [1, 2, 4, 8, 16, 32, 64, 64, 64, 64, 64, 64, 64]
    );
    assert_eq!(TCP_TOTBACKOFF, 511);
    assert_eq!(TCP_MAXRXTSHIFT, 12);
    assert_eq!(TCPTV_MIN_MS, 500);
    assert_eq!(TCPTV_REXMTMAX_MS, 64_000);
    assert_eq!(TCPTV_RTOBASE_MS, 1_000);
    assert_eq!(TCPTV_MSL_MS, 30_000);
    assert_eq!(TCPTV_KEEP_INIT_MS, 75_000);
    assert_eq!(TCPTV_KEEP_IDLE_MS, 7_200_000);
    assert_eq!(TCPTV_KEEPINTVL_MS, 75_000);
    assert_eq!(TCPTV_KEEPCNT, 8);
}

#[test]
fn a_new_timer_state_uses_the_netbsd_defaults() {
    let tp = TimerState::new(TcpState::Established, 123);
    assert_eq!(tp.rcvtime_ms, 123);
    assert_eq!(tp.rttmin_ms, 500);
    assert_eq!(tp.rxtcur_ms, 1_000);
    assert_eq!(tp.keepinit_ms, 75_000);
    assert_eq!(tp.keepidle_ms, 7_200_000);
    assert_eq!(tp.keepintvl_ms, 75_000);
    assert_eq!(tp.keepcnt, 8);
    assert_eq!(tp.maxidle_ms(), 600_000);
    assert_eq!(tp.rtt.rto_ms(), 1_000);
    assert_eq!(tp.rtt.rexmtval_ms(), 1_000);
    assert!(tp.mtudisc);
    assert!(!tp.closed);
    assert!(!tp.keepalive);
    assert!(!tp.syn_rexmt);
    assert_eq!(tp.soft_error, None);
}

#[test]
fn the_rto_backoff_uses_the_netbsd_table_without_cpu_slop() {
    let mut tp = state(TcpState::Established);
    tp.rtt.sample(400);
    assert_eq!(tp.rtt.rexmtval_ms(), 1_200);
    let expected: [(u8, u64); 8] = [
        (1, 2_400),
        (2, 4_800),
        (3, 9_600),
        (4, 19_200),
        (5, 38_400),
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
fn the_initial_rto_is_one_second_and_the_first_arm_is_two_seconds() {
    let mut tp = state(TcpState::Established);
    let action = retransmit(&mut tp, 1_000);
    assert_eq!(tp.rxtcur_ms, 2_000);
    assert_eq!(tp.rexmt_deadline_ms, Some(3_000));
    let RexmtAction::Retransmit { rxtcur_ms, .. } = action else {
        unreachable!();
    };
    assert_eq!(rxtcur_ms, 2_000);
    let _ = retransmit(&mut tp, 3_000);
    assert_eq!(tp.rxtcur_ms, 4_000);
}

#[test]
fn a_syn_retransmit_sets_the_syn_flag_and_uses_the_rto_base() {
    let mut tp = state(TcpState::SynSent);
    let action = retransmit(&mut tp, 1_000);
    let RexmtAction::Retransmit { syn, rxtcur_ms, .. } = action else {
        unreachable!();
    };
    assert!(syn);
    assert_eq!(rxtcur_ms, 2_000);
    assert!(tp.syn_rexmt);
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
fn a_softerror_is_reported_instead_of_etimedout() {
    let mut tp = state(TcpState::Established);
    tp.soft_error = Some(Errno::ECONNRESET);
    tp.rxtshift = TCP_MAXRXTSHIFT;
    let outcome = fire_rexmt(&mut tp, 1_000);
    assert_eq!(
        outcome.action,
        RexmtAction::Drop {
            error: Errno::ECONNRESET
        }
    );
    assert_eq!(tp.soft_error, None);
}

#[test]
fn every_retransmit_clears_the_sack_scoreboard_and_resets_fack() {
    let mut tp = state(TcpState::Established);
    tp.sackholes = 3;
    tp.snd_una = 5_000;
    tp.snd_fack = 9_000;
    let action = retransmit(&mut tp, 1_000);
    let RexmtAction::Retransmit { free_sackholes, .. } = action else {
        unreachable!();
    };
    assert!(free_sackholes);
    assert_eq!(tp.sackholes, 0);
    assert_eq!(tp.snd_fack, 5_000);

    tp.sackholes = 2;
    tp.snd_fack = 8_000;
    let _ = retransmit(&mut tp, 3_000);
    assert_eq!(tp.sackholes, 0, "every timeout clears, not only the first");
    assert_eq!(tp.snd_fack, 5_000);
}

#[test]
fn rexmt_rewinds_the_send_pointer_and_stops_the_rtt_timer() {
    let mut tp = state(TcpState::Established);
    tp.snd_una = 5_000;
    tp.snd_nxt = 5_005;
    tp.snd_max = 6_000;
    tp.snd_high = 5_500;
    tp.rtttime_active = true;
    let action = retransmit(&mut tp, 1_000);
    let RexmtAction::Retransmit { syn, .. } = action else {
        unreachable!();
    };
    assert!(!syn);
    assert_eq!(tp.snd_nxt, 5_000);
    assert_eq!(tp.snd_high, 6_000);
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
fn pmtud_is_turned_off_after_the_third_retransmit() {
    for shift in 1u8..=2 {
        let mut tp = state(TcpState::Established);
        tp.rxtshift = shift - 1;
        let action = retransmit(&mut tp, 1_000);
        let RexmtAction::Retransmit {
            mtudisc_disabled, ..
        } = action
        else {
            unreachable!();
        };
        assert!(!mtudisc_disabled, "shift {shift}");
        assert!(tp.mtudisc, "shift {shift}");
    }
    let mut tp = state(TcpState::Established);
    tp.rxtshift = 2;
    let action = retransmit(&mut tp, 1_000);
    let RexmtAction::Retransmit {
        mtudisc_disabled, ..
    } = action
    else {
        unreachable!();
    };
    assert!(mtudisc_disabled);
    assert!(!tp.mtudisc);
    // A later retransmit reports no change: it was disabled once.
    let action = retransmit(&mut tp, 3_000);
    let RexmtAction::Retransmit {
        mtudisc_disabled, ..
    } = action
    else {
        unreachable!();
    };
    assert!(!mtudisc_disabled);
}

#[test]
fn rtt_is_folded_into_rttvar_after_the_fourth_retransmit() {
    let mut tp = state(TcpState::Established);
    tp.rtt.sample(400);
    assert_eq!(tp.rtt.srtt_ms(), 400);
    assert_eq!(tp.rtt.rttvar_ms(), 200);
    for shift in 1u8..=3 {
        let action = retransmit(&mut tp, u64::from(shift) * 1_000);
        let RexmtAction::Retransmit { losing, .. } = action else {
            unreachable!();
        };
        assert!(!losing, "shift {shift}");
    }
    assert_eq!(tp.rtt.srtt_ms(), 400);
    assert_eq!(tp.rtt.rttvar_ms(), 200);
    let action = retransmit(&mut tp, 4_000);
    let RexmtAction::Retransmit { losing, .. } = action else {
        unreachable!();
    };
    assert!(losing);
    // NetBSD keeps srtt + 4*rttvar constant: 400 + 4*300 == 400 + 4*200.
    assert_eq!(tp.rtt.srtt_ms(), 0);
    assert_eq!(tp.rtt.rttvar_ms(), 300);
    assert_eq!(tp.rtt.rexmtval_ms(), 1_200);
    tp.rtt.sample(200);
    assert_eq!(tp.rtt.srtt_ms(), 200);
    assert_eq!(tp.rtt.rttvar_ms(), 100);
    assert_eq!(tp.rtt.rexmtval_ms(), 600);
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
fn keep_probes_immediately_and_does_not_early_rearm() {
    // NetBSD has no FreeBSD-style early rearm: with SO_KEEPALIVE and an idle time below
    // keepidle+maxidle, an expired callout probes at once and rearms to keepintvl.
    let mut tp = established_keep();
    tp.rcvtime_ms = 0;
    let outcome = tcp_timer_keep(&mut tp, 1_000);
    assert_eq!(
        outcome,
        KeepOutcome::Probe {
            seq: 999,
            ack: 2_000
        }
    );
    assert_eq!(tp.keep_deadline_ms, Some(76_000));
}

#[test]
fn keepalive_probes_with_snd_una_minus_one() {
    let mut tp = established_keep();
    tp.rcvtime_ms = 0;
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
fn keep_rearms_to_keepidle_without_the_socket_option() {
    let mut tp = established_keep();
    tp.keepalive = false;
    assert_eq!(tcp_timer_keep(&mut tp, 7_200_000), KeepOutcome::Rearm);
    assert_eq!(tp.keep_deadline_ms, Some(14_400_000));
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
fn keep_probes_only_through_close_wait() {
    for kind in [TcpState::Established, TcpState::CloseWait] {
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
    for kind in [
        TcpState::FinWait1,
        TcpState::FinWait2,
        TcpState::Closing,
        TcpState::LastAck,
        TcpState::TimeWait,
    ] {
        let mut tp = established_keep();
        tp.state = kind;
        assert_eq!(
            tcp_timer_keep(&mut tp, 7_200_000),
            KeepOutcome::Rearm,
            "{kind:?}"
        );
        assert_eq!(tp.keep_deadline_ms, Some(14_400_000), "{kind:?}");
    }
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
    tp.snd_fack = 9_000;
    assert_eq!(tcp_timer_2msl(&mut tp, 60_000), TwoMslOutcome::Close);
    assert!(tp.closed);
    assert_eq!(tp.two_msl_deadline_ms, None);
    assert_eq!(tp.sackholes, 0);
    assert_eq!(tp.snd_fack, tp.snd_una);
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
fn arm_two_msl_uses_two_msl() {
    let mut tp = TimerState::new(TcpState::TimeWait, 0);
    tp.arm_two_msl(123, 30_000);
    assert_eq!(tp.two_msl_deadline_ms, Some(60_123));
}

#[test]
fn zero_keepalive_options_use_the_netbsd_invariants() {
    let mut tp = TimerState::new(TcpState::Established, 0);
    tp.keepinit_ms = 0;
    tp.keepidle_ms = 0;
    tp.keepintvl_ms = 0;
    tp.keepcnt = 0;
    assert_eq!(tp.effective_keepinit_ms(), 75_000);
    assert_eq!(tp.effective_keepidle_ms(), 7_200_000);
    assert_eq!(tp.effective_keepintvl_ms(), 75_000);
    // NetBSD keeps t_keepcnt >= 1 in tcp_tcpcb_template(): MAX(1, MIN(tcp_keepcnt, ...)).
    assert_eq!(tp.maxidle_ms(), 75_000);
}

#[test]
fn arm_keep_init_uses_the_connect_interval() {
    let mut tp = TimerState::new(TcpState::SynSent, 0);
    tp.arm_keep_init(500);
    assert_eq!(tp.keep_deadline_ms, Some(75_500));
}

#[test]
fn the_state_predicates_match_the_netbsd_macros() {
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
    for kind in [TcpState::Established, TcpState::CloseWait] {
        assert!(keep_state_allows_probe(kind), "{kind:?}");
    }
    for kind in [
        TcpState::Closed,
        TcpState::Listen,
        TcpState::SynSent,
        TcpState::SynReceived,
        TcpState::FinWait1,
        TcpState::FinWait2,
        TcpState::Closing,
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

// ---------------------------------------------------------------------------
// Randomized host model vs the merged NetBSD timer state.
//
// The product timer state is private to this module, so the model lives in the
// unit-test file and drives the exact merged `TimerState` / entry points. The
// reference below is an independent integer implementation of the NetBSD
// equations; it is not a second call into the product.
// ---------------------------------------------------------------------------

const MODEL_STATES: [TcpState; 11] = [
    TcpState::Closed,
    TcpState::Listen,
    TcpState::SynSent,
    TcpState::SynReceived,
    TcpState::Established,
    TcpState::CloseWait,
    TcpState::FinWait1,
    TcpState::Closing,
    TcpState::LastAck,
    TcpState::FinWait2,
    TcpState::TimeWait,
];

const REF_BACKOFF: [u32; 13] = [1, 2, 4, 8, 16, 32, 64, 64, 64, 64, 64, 64, 64];

struct ModelRng(u64);

impl ModelRng {
    const fn new(seed: u64) -> Self {
        Self(if seed == 0 {
            0x9e37_79b9_7f4a_7c15
        } else {
            seed
        })
    }

    fn next_u32(&mut self) -> u32 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        (value >> 16) as u32
    }

    fn below(&mut self, bound: u32) -> u32 {
        if bound == 0 {
            0
        } else {
            self.next_u32() % bound
        }
    }

    fn range(&mut self, low: u32, high: u32) -> u32 {
        if high <= low {
            low
        } else {
            low + self.below(high - low + 1)
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct RefRto {
    srtt_q8: u64,
    rttvar_q8: u64,
    has_sample: bool,
    invalid: bool,
    rto_ms: u64,
    last_computed_ms: u64,
    initial_ms: u64,
    min_ms: u64,
    max_ms: u64,
    granularity_ms: u64,
}

impl RefRto {
    fn new(initial_ms: u64, min_ms: u64, max_ms: u64, granularity_ms: u64) -> Self {
        let initial_ms = initial_ms.clamp(min_ms, max_ms);
        Self {
            srtt_q8: 0,
            rttvar_q8: 0,
            has_sample: false,
            invalid: false,
            rto_ms: initial_ms,
            last_computed_ms: initial_ms,
            initial_ms,
            min_ms,
            max_ms,
            granularity_ms: granularity_ms.max(1),
        }
    }

    fn sample(&mut self, rtt_ms: u64) {
        let rq = rtt_ms.min(86_400_000).saturating_mul(8);
        if !self.has_sample || self.invalid {
            self.srtt_q8 = rq;
            self.rttvar_q8 = rq / 2;
            self.has_sample = true;
            self.invalid = false;
        } else {
            let diff = self.srtt_q8.abs_diff(rq);
            self.rttvar_q8 = ref_ceil_div(self.rttvar_q8.saturating_mul(3).saturating_add(diff), 4);
            self.srtt_q8 = ref_ceil_div(self.srtt_q8.saturating_mul(7).saturating_add(rq), 8);
        }
        let margin = self
            .granularity_ms
            .saturating_mul(8)
            .max(self.rttvar_q8.saturating_mul(4));
        let computed = ref_ceil_div(self.srtt_q8.saturating_add(margin), 8);
        self.last_computed_ms = computed;
        self.rto_ms = computed.clamp(self.min_ms, self.max_ms);
    }

    fn timeout(&mut self) {
        self.rto_ms = self.rto_ms.saturating_mul(2).min(self.max_ms);
    }

    fn losing(&mut self) {
        self.rttvar_q8 = self.rttvar_q8.saturating_add(self.srtt_q8 / 4);
        self.srtt_q8 = 0;
        self.invalid = true;
    }

    fn rexmtval(&self) -> u64 {
        if self.has_sample {
            ref_ceil_div(
                self.srtt_q8
                    .saturating_add(self.rttvar_q8.saturating_mul(4)),
                8,
            )
        } else {
            self.initial_ms
        }
    }

    fn backoff(&self, shift: u8, min_ms: u64, max_ms: u64) -> u64 {
        Self::backoff_from(self.rexmtval(), shift, min_ms, max_ms)
    }

    fn backoff_from(base_ms: u64, shift: u8, min_ms: u64, max_ms: u64) -> u64 {
        let base = base_ms.max(min_ms);
        let factor = REF_BACKOFF[usize::from(shift).min(12)];
        let value = base.saturating_mul(u64::from(factor));
        if value < min_ms {
            min_ms
        } else if value > max_ms {
            max_ms
        } else {
            value
        }
    }
}

#[allow(clippy::manual_is_multiple_of)] // explicit remainder form mirrors the donor equation
fn ref_ceil_div(value: u64, divisor: u64) -> u64 {
    if divisor == 0 {
        value
    } else if value % divisor == 0 {
        value / divisor
    } else {
        value / divisor + 1
    }
}

fn ref_established(state: TcpState) -> bool {
    matches!(
        state,
        TcpState::Established
            | TcpState::FinWait1
            | TcpState::FinWait2
            | TcpState::CloseWait
            | TcpState::Closing
            | TcpState::LastAck
            | TcpState::TimeWait
    )
}

fn ref_keep_state_allows_probe(state: TcpState) -> bool {
    // NetBSD `t_state <= TCPS_CLOSE_WAIT`: only ESTABLISHED (4) and CLOSE_WAIT (5).
    matches!(state, TcpState::Established | TcpState::CloseWait)
}

fn ref_keepidle(tp: &TimerState) -> u64 {
    if tp.keepidle_ms == 0 {
        TCPTV_KEEP_IDLE_MS
    } else {
        tp.keepidle_ms
    }
}

fn ref_keepintvl(tp: &TimerState) -> u64 {
    if tp.keepintvl_ms == 0 {
        TCPTV_KEEPINTVL_MS
    } else {
        tp.keepintvl_ms
    }
}

fn ref_maxidle(tp: &TimerState) -> u64 {
    // `tcp_tcpcb_template()` keeps `t_keepcnt >= 1`; `t_maxidle = keepcnt * keepintvl`.
    tp.keepcnt.max(1).saturating_mul(ref_keepintvl(tp))
}

fn ref_drop(tp: &mut TimerState) {
    tp.closed = true;
    tp.rexmt_deadline_ms = None;
    tp.keep_deadline_ms = None;
    tp.two_msl_deadline_ms = None;
}

fn ref_rexmt(tp: &mut TimerState, now_ms: u64) -> RexmtOutcome {
    let ignored = RexmtOutcome {
        action: RexmtAction::Ignored,
    };
    if tp.closed || !callout_expired(tp.rexmt_deadline_ms, now_ms) {
        return ignored;
    }

    tp.sackholes = 0;
    tp.snd_fack = tp.snd_una;

    tp.rxtshift = tp.rxtshift.saturating_add(1);
    if tp.rxtshift > TCP_MAXRXTSHIFT {
        tp.rxtshift = TCP_MAXRXTSHIFT;
        let error = tp.soft_error.take().unwrap_or(Errno::ETIMEDOUT);
        ref_drop(tp);
        return RexmtOutcome {
            action: RexmtAction::Drop { error },
        };
    }

    let mut base = tp.rtt.rexmtval_ms();
    if base < tp.rttmin_ms {
        base = tp.rttmin_ms;
    }
    tp.rxtcur_ms = RefRto::backoff_from(base, tp.rxtshift, tp.rttmin_ms, TCPTV_REXMTMAX_MS);
    tp.rexmt_deadline_ms = Some(now_ms.saturating_add(tp.rxtcur_ms));

    let mtudisc_disabled = tp.mtudisc && tp.rxtshift > TCP_MAXRXTSHIFT / 6;
    if mtudisc_disabled {
        tp.mtudisc = false;
    }
    let losing = tp.rxtshift > TCP_MAXRXTSHIFT / 4;
    if losing {
        tp.rtt.losing();
    }

    tp.snd_nxt = tp.snd_una;
    tp.snd_high = tp.snd_max;
    tp.rtttime_active = false;
    let syn = tp.state == TcpState::SynSent;
    if syn {
        tp.syn_rexmt = true;
    }

    RexmtOutcome {
        action: RexmtAction::Retransmit {
            rxtcur_ms: tp.rxtcur_ms,
            syn,
            free_sackholes: true,
            mtudisc_disabled,
            losing,
        },
    }
}

fn ref_keep(tp: &mut TimerState, now_ms: u64) -> KeepOutcome {
    if tp.closed || !callout_expired(tp.keep_deadline_ms, now_ms) {
        return KeepOutcome::Ignored;
    }

    if !ref_established(tp.state) {
        ref_drop(tp);
        return KeepOutcome::Drop {
            error: Errno::ETIMEDOUT,
        };
    }

    if tp.keepalive && ref_keep_state_allows_probe(tp.state) {
        let maxidle = ref_maxidle(tp);
        if maxidle > 0
            && now_ms.saturating_sub(tp.rcvtime_ms) >= ref_keepidle(tp).saturating_add(maxidle)
        {
            ref_drop(tp);
            return KeepOutcome::Drop {
                error: Errno::ETIMEDOUT,
            };
        }
        tp.keep_deadline_ms = Some(now_ms.saturating_add(ref_keepintvl(tp)));
        return KeepOutcome::Probe {
            seq: tp.snd_una.wrapping_sub(1),
            ack: tp.rcv_nxt,
        };
    }

    tp.keep_deadline_ms = Some(now_ms.saturating_add(ref_keepidle(tp)));
    KeepOutcome::Rearm
}

fn ref_two_msl(tp: &mut TimerState, now_ms: u64) -> TwoMslOutcome {
    if tp.closed || !callout_expired(tp.two_msl_deadline_ms, now_ms) {
        return TwoMslOutcome::Ignored;
    }

    tp.sackholes = 0;
    tp.snd_fack = tp.snd_una;
    let maxidle = ref_maxidle(tp);
    if tp.state != TcpState::TimeWait
        && (maxidle == 0 || now_ms.saturating_sub(tp.rcvtime_ms) <= maxidle)
    {
        tp.two_msl_deadline_ms = Some(now_ms.saturating_add(ref_keepintvl(tp)));
        TwoMslOutcome::Rearm
    } else {
        ref_drop(tp);
        TwoMslOutcome::Close
    }
}

fn random_rto(rng: &mut ModelRng) -> (RtoEstimator, RefRto) {
    let initial = u64::from(rng.range(200, 3_000));
    let min = u64::from(rng.range(1, 500));
    let max = u64::from(rng.range(3_000, 64_000)).max(min.saturating_add(1));
    let granularity = u64::from(rng.range(1, 100));
    let mut product = RtoEstimator::new(initial, min, max, granularity);
    let mut reference = RefRto::new(initial, min, max, granularity);
    for _ in 0..rng.range(0, 5) {
        match rng.below(5) {
            0 => {
                product.timeout();
                reference.timeout();
            }
            1 => {
                product.losing();
                reference.losing();
            }
            _ => {
                let sample = u64::from(rng.range(1, 5_000));
                product.sample(sample);
                reference.sample(sample);
            }
        }
    }
    (product, reference)
}

fn random_deadline(rng: &mut ModelRng, now_ms: u64) -> Option<u64> {
    match rng.below(4) {
        0 => None,
        1 => Some(now_ms.saturating_sub(u64::from(rng.range(0, 1_000)))),
        2 => Some(now_ms),
        _ => Some(now_ms.saturating_add(u64::from(rng.range(1, 200_000)))),
    }
}

fn random_timer(rng: &mut ModelRng, now_ms: u64) -> TimerState {
    let state = MODEL_STATES[usize::try_from(rng.below(11)).unwrap_or(0)];
    let mut tp = TimerState::new(state, now_ms);
    tp.closed = rng.below(20) == 0;
    tp.keepalive = rng.below(2) == 0;
    tp.sackholes = usize::try_from(rng.below(8)).unwrap_or(0);
    tp.snd_una = rng.next_u32();
    tp.snd_nxt = tp.snd_una.wrapping_add(rng.below(1_000));
    tp.snd_max = tp.snd_nxt.wrapping_add(rng.below(1_000));
    tp.snd_high = tp.snd_max.wrapping_add(rng.below(1_000));
    tp.snd_fack = tp.snd_una.wrapping_add(rng.below(1_000));
    tp.snd_recover = rng.next_u32();
    tp.rcv_nxt = rng.next_u32();
    tp.rcvtime_ms = now_ms.saturating_sub(u64::from(rng.range(0, 500_000)));
    tp.rttmin_ms = u64::from(rng.range(1, 500));
    tp.rxtshift = u8::try_from(rng.range(0, 13)).unwrap_or(TCP_MAXRXTSHIFT);
    tp.rxtcur_ms = u64::from(rng.range(1, 64_000));
    tp.rtttime_active = rng.below(2) == 0;
    tp.mtudisc = rng.below(2) == 0;
    tp.soft_error = match rng.below(4) {
        0 => None,
        1 => Some(Errno::ETIMEDOUT),
        2 => Some(Errno::ECONNRESET),
        _ => Some(Errno::EHOSTUNREACH),
    };
    tp.syn_rexmt = rng.below(2) == 0;
    tp.keepinit_ms = if rng.below(4) == 0 {
        0
    } else {
        u64::from(rng.range(1, 200_000))
    };
    tp.keepidle_ms = if rng.below(4) == 0 {
        0
    } else {
        u64::from(rng.range(1, 500_000))
    };
    tp.keepintvl_ms = if rng.below(4) == 0 {
        0
    } else {
        u64::from(rng.range(1, 200_000))
    };
    tp.keepcnt = if rng.below(4) == 0 {
        0
    } else {
        u64::from(rng.range(1, 16))
    };
    tp.rexmt_deadline_ms = random_deadline(rng, now_ms);
    tp.keep_deadline_ms = random_deadline(rng, now_ms);
    tp.two_msl_deadline_ms = random_deadline(rng, now_ms);
    let (rto, _) = random_rto(rng);
    tp.rtt = rto;
    tp
}

fn assert_timer_eq(actual: &TimerState, expected: &TimerState, context: &str) {
    macro_rules! check {
        ($field:ident) => {
            assert_eq!(
                actual.$field,
                expected.$field,
                "{context}: {} differs",
                stringify!($field)
            );
        };
    }
    check!(state);
    check!(closed);
    check!(keepalive);
    check!(sackholes);
    check!(snd_una);
    check!(snd_nxt);
    check!(snd_max);
    check!(snd_high);
    check!(snd_fack);
    check!(snd_recover);
    check!(rcv_nxt);
    check!(rcvtime_ms);
    check!(rttmin_ms);
    check!(rxtshift);
    check!(rxtcur_ms);
    check!(rtttime_active);
    check!(mtudisc);
    check!(soft_error);
    check!(syn_rexmt);
    check!(keepinit_ms);
    check!(keepidle_ms);
    check!(keepintvl_ms);
    check!(keepcnt);
    check!(rexmt_deadline_ms);
    check!(keep_deadline_ms);
    check!(two_msl_deadline_ms);
    assert_eq!(
        actual.rtt.srtt_q8(),
        expected.rtt.srtt_q8(),
        "{context}: srtt_q8 differs"
    );
    assert_eq!(
        actual.rtt.rttvar_q8(),
        expected.rtt.rttvar_q8(),
        "{context}: rttvar_q8 differs"
    );
    assert_eq!(
        actual.rtt.has_sample(),
        expected.rtt.has_sample(),
        "{context}: has_sample differs"
    );
    assert_eq!(
        actual.rtt.rto_ms(),
        expected.rtt.rto_ms(),
        "{context}: rto_ms differs"
    );
    assert_eq!(
        actual.rtt.rto_computed_ms(),
        expected.rtt.rto_computed_ms(),
        "{context}: rto_computed_ms differs"
    );
    assert_eq!(
        actual.rtt.rexmtval_ms(),
        expected.rtt.rexmtval_ms(),
        "{context}: rexmtval_ms differs"
    );
}

#[test]
fn randomized_rto_estimator_matches_reference_for_10k_operations() {
    let mut rng = ModelRng::new(0x309_0001);
    let mut product = RtoEstimator::new(1_000, 500, 64_000, 1);
    let mut reference = RefRto::new(1_000, 500, 64_000, 1);
    for step in 0..10_000 {
        match rng.below(13) {
            0 => {
                product.timeout();
                reference.timeout();
            }
            1 => {
                product.losing();
                reference.losing();
            }
            _ => {
                let sample = u64::from(rng.range(1, 5_000));
                product.sample(sample);
                reference.sample(sample);
            }
        }
        assert_eq!(product.srtt_q8(), reference.srtt_q8, "srtt step {step}");
        assert_eq!(
            product.rttvar_q8(),
            reference.rttvar_q8,
            "rttvar step {step}"
        );
        assert_eq!(product.rto_ms(), reference.rto_ms, "rto step {step}");
        assert_eq!(
            product.rto_computed_ms(),
            reference.last_computed_ms,
            "computed step {step}"
        );
        assert_eq!(
            product.rexmtval_ms(),
            reference.rexmtval(),
            "rexmtval step {step}"
        );
        let shift = u8::try_from(rng.range(0, 12)).unwrap_or(12);
        let min = u64::from(rng.range(1, 500));
        let max = u64::from(rng.range(3_000, 64_000)).max(min + 1);
        assert_eq!(
            product.backoff_ms(shift, min, max),
            reference.backoff(shift, min, max),
            "backoff step {step}"
        );
    }
}

#[test]
fn randomized_rexmt_timer_matches_reference_for_10k_operations() {
    let mut rng = ModelRng::new(0x309_0002);
    for step in 0..10_000 {
        let now = u64::from(rng.range(0, 1_000_000));
        let mut actual = random_timer(&mut rng, now);
        let mut expected = actual.clone();
        let actual_outcome = tcp_timer_rexmt(&mut actual, now);
        let expected_outcome = ref_rexmt(&mut expected, now);
        assert_eq!(
            actual_outcome, expected_outcome,
            "rexmt outcome step {step}"
        );
        assert_timer_eq(&actual, &expected, &format!("rexmt step {step}"));
    }
}

#[test]
fn randomized_keepalive_timer_matches_reference_for_10k_operations() {
    let mut rng = ModelRng::new(0x309_0003);
    for step in 0..10_000 {
        let now = u64::from(rng.range(0, 1_000_000));
        let mut actual = random_timer(&mut rng, now);
        let mut expected = actual.clone();
        let actual_outcome = tcp_timer_keep(&mut actual, now);
        let expected_outcome = ref_keep(&mut expected, now);
        assert_eq!(
            actual_outcome, expected_outcome,
            "keepalive outcome step {step}"
        );
        assert_timer_eq(&actual, &expected, &format!("keepalive step {step}"));
    }
}

#[test]
fn randomized_two_msl_timer_matches_reference_for_10k_operations() {
    let mut rng = ModelRng::new(0x309_0004);
    for step in 0..10_000 {
        let now = u64::from(rng.range(0, 1_000_000));
        let mut actual = random_timer(&mut rng, now);
        let mut expected = actual.clone();
        let actual_outcome = tcp_timer_2msl(&mut actual, now);
        let expected_outcome = ref_two_msl(&mut expected, now);
        assert_eq!(actual_outcome, expected_outcome, "2MSL outcome step {step}");
        assert_timer_eq(&actual, &expected, &format!("2MSL step {step}"));
    }
}
