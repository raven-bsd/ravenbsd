// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the FreeBSD timer port (K267).

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

// ---------------------------------------------------------------------------
// Randomized host model vs the merged timer state.
//
// The product timer state is private to this module, so the model lives in the
// unit-test file and drives the exact merged `TimerState` / entry points. The
// reference below is an independent integer implementation of the FreeBSD
// equations recorded by K267; it is not a second call into the product.
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

const REF_BACKOFF: [u32; 13] = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 512, 512, 512];

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

    fn invalidate(&mut self) {
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

    fn backoff(&self, shift: u8, min_ms: u64, slop_ms: u64, max_ms: u64) -> u64 {
        Self::backoff_from(self.rexmtval(), shift, min_ms, slop_ms, max_ms)
    }

    fn backoff_from(base_ms: u64, shift: u8, min_ms: u64, slop_ms: u64, max_ms: u64) -> u64 {
        let base = base_ms.max(min_ms);
        let factor = REF_BACKOFF[usize::from(shift).min(12)];
        let value = base
            .saturating_mul(u64::from(factor))
            .saturating_add(slop_ms);
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
    matches!(
        state,
        TcpState::Established
            | TcpState::FinWait1
            | TcpState::FinWait2
            | TcpState::CloseWait
            | TcpState::Closing
    )
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

fn ref_keepcnt(tp: &TimerState) -> u64 {
    if tp.keepcnt == 0 {
        TCPTV_KEEPCNT
    } else {
        tp.keepcnt
    }
}

fn ref_maxidle(tp: &TimerState) -> u64 {
    ref_keepcnt(tp).saturating_mul(ref_keepintvl(tp))
}

fn ref_maxunack(tp: &TimerState, now_ms: u64) -> bool {
    tp.maxunacktime_ms != 0
        && tp.acktime_ms != 0
        && now_ms >= tp.acktime_ms.saturating_add(tp.maxunacktime_ms)
}

fn ref_drop(tp: &mut TimerState) {
    tp.closed = true;
    tp.rexmt_deadline_ms = None;
    tp.keep_deadline_ms = None;
    tp.two_msl_deadline_ms = None;
}

fn ref_rexmt(tp: &mut TimerState, ref_rto: &mut RefRto, now_ms: u64) -> RexmtOutcome {
    let ignored = RexmtOutcome {
        action: RexmtAction::Ignored,
    };
    if tp.closed || !callout_expired(tp.rexmt_deadline_ms, now_ms) {
        return ignored;
    }

    tp.rxtshift = tp.rxtshift.saturating_add(1);
    if tp.rxtshift > tp.retries || ref_maxunack(tp, now_ms) {
        tp.rxtshift = tp.retries;
        ref_drop(tp);
        return RexmtOutcome {
            action: RexmtAction::Drop {
                error: Errno::ETIMEDOUT,
            },
        };
    }

    let syn = matches!(tp.state, TcpState::SynSent | TcpState::SynReceived);
    let first = tp.rxtshift == 1;
    let resend_sackholes = first;
    if !first {
        tp.sackholes = 0;
    }
    let base = if syn {
        tp.rexmit_initial_ms
    } else {
        ref_rto.rexmtval()
    };
    tp.rxtcur_ms = RefRto::backoff_from(
        base,
        tp.rxtshift,
        tp.rttmin_ms,
        tp.rexmit_slop_ms,
        tp.rexmit_max_ms,
    );
    tp.rexmt_deadline_ms = Some(now_ms.saturating_add(tp.rxtcur_ms));

    let rtt_invalidated = tp.rxtshift > TCP_RTT_INVALIDATE;
    if rtt_invalidated {
        ref_rto.invalidate();
        tp.rtt.invalidate_srtt();
    }
    let blackhole_detect = first && matches!(tp.state, TcpState::Established | TcpState::FinWait1);
    let drop_options = tp.rexmit_drop_options && tp.state == TcpState::SynSent && tp.rxtshift == 3;

    tp.snd_nxt = tp.snd_una;
    tp.snd_recover = tp.snd_max;
    tp.acknow = true;
    tp.rtttime_active = false;

    RexmtOutcome {
        action: RexmtAction::Retransmit {
            rxtcur_ms: tp.rxtcur_ms,
            syn,
            first,
            resend_sackholes,
            drop_options,
            rtt_invalidated,
            blackhole_detect,
        },
    }
}

fn ref_keep(tp: &mut TimerState, now_ms: u64) -> KeepOutcome {
    if tp.closed || !callout_expired(tp.keep_deadline_ms, now_ms) {
        return KeepOutcome::Ignored;
    }

    if ref_established(tp.state) {
        let idle = now_ms.saturating_sub(tp.rcvtime_ms);
        let keepidle = ref_keepidle(tp);
        if idle < keepidle {
            tp.keep_deadline_ms = Some(now_ms.saturating_add(keepidle - idle));
            return KeepOutcome::Rearm;
        }
    }

    if !ref_established(tp.state) {
        ref_drop(tp);
        return KeepOutcome::Drop {
            error: Errno::ETIMEDOUT,
        };
    }

    if (tp.always_keepalive || tp.keepalive) && ref_keep_state_allows_probe(tp.state) {
        if now_ms.saturating_sub(tp.rcvtime_ms) >= ref_keepidle(tp).saturating_add(ref_maxidle(tp))
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
    let force_close = tp.state == TcpState::TimeWait
        || (tp.state == TcpState::FinWait2 && tp.fast_finwait2_recycle && tp.receiver_closed);
    let close = if force_close {
        true
    } else if now_ms.saturating_sub(tp.rcvtime_ms) <= ref_maxidle(tp) {
        tp.two_msl_deadline_ms = Some(now_ms.saturating_add(ref_keepintvl(tp)));
        false
    } else {
        true
    };
    if close {
        ref_drop(tp);
        TwoMslOutcome::Close
    } else {
        TwoMslOutcome::Rearm
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
        if rng.below(3) == 0 {
            product.timeout();
            reference.timeout();
        } else {
            let sample = u64::from(rng.range(1, 5_000));
            product.sample(sample);
            reference.sample(sample);
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
    tp.always_keepalive = rng.below(2) == 0;
    tp.keepalive = rng.below(2) == 0;
    tp.receiver_closed = rng.below(2) == 0;
    tp.fast_finwait2_recycle = rng.below(2) == 0;
    tp.sackholes = usize::try_from(rng.below(8)).unwrap_or(0);
    tp.snd_una = rng.next_u32();
    tp.snd_nxt = tp.snd_una.wrapping_add(rng.below(1_000));
    tp.snd_max = tp.snd_nxt.wrapping_add(rng.below(1_000));
    tp.snd_recover = rng.next_u32();
    tp.rcv_nxt = rng.next_u32();
    tp.acktime_ms = if rng.below(3) == 0 {
        0
    } else {
        now_ms.saturating_sub(u64::from(rng.range(0, 100_000)))
    };
    tp.rcvtime_ms = now_ms.saturating_sub(u64::from(rng.range(0, 500_000)));
    tp.rttmin_ms = u64::from(rng.range(5, 500));
    tp.rexmit_slop_ms = u64::from(rng.range(0, 500));
    tp.rexmit_initial_ms = u64::from(rng.range(200, 3_000));
    tp.rexmit_max_ms = u64::from(rng.range(3_000, 64_000));
    tp.retries = u8::try_from(rng.range(1, 12)).unwrap_or(12);
    tp.rexmit_drop_options = rng.below(2) == 0;
    tp.maxunacktime_ms = if rng.below(3) == 0 {
        0
    } else {
        u64::from(rng.range(1, 200_000))
    };
    tp.rxtshift = u8::try_from(rng.range(0, u32::from(tp.retries) + 1)).unwrap_or(tp.retries);
    tp.rxtcur_ms = u64::from(rng.range(1, 64_000));
    tp.rtttime_active = rng.below(2) == 0;
    tp.acknow = rng.below(2) == 0;
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
    check!(always_keepalive);
    check!(keepalive);
    check!(receiver_closed);
    check!(fast_finwait2_recycle);
    check!(sackholes);
    check!(snd_una);
    check!(snd_nxt);
    check!(snd_max);
    check!(snd_recover);
    check!(rcv_nxt);
    check!(acktime_ms);
    check!(rcvtime_ms);
    check!(rttmin_ms);
    check!(rexmit_slop_ms);
    check!(rexmit_initial_ms);
    check!(rexmit_max_ms);
    check!(retries);
    check!(rexmit_drop_options);
    check!(maxunacktime_ms);
    check!(rxtshift);
    check!(rxtcur_ms);
    check!(rtttime_active);
    check!(acknow);
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
    let mut product = RtoEstimator::new(1_000, 30, 64_000, 1);
    let mut reference = RefRto::new(1_000, 30, 64_000, 1);
    for step in 0..10_000 {
        if rng.below(4) == 0 {
            product.timeout();
            reference.timeout();
        } else {
            let sample = u64::from(rng.range(1, 5_000));
            product.sample(sample);
            reference.sample(sample);
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
        let slop = u64::from(rng.range(0, 500));
        let max = u64::from(rng.range(3_000, 64_000)).max(min + 1);
        assert_eq!(
            product.backoff_ms(shift, min, slop, max),
            reference.backoff(shift, min, slop, max),
            "backoff step {step}"
        );
        if rng.below(13) == 0 {
            product.invalidate_srtt();
            reference.invalidate();
        }
    }
}

#[test]
fn randomized_rexmt_timer_matches_reference_for_10k_operations() {
    let mut rng = ModelRng::new(0x309_0002);
    for step in 0..10_000 {
        let now = u64::from(rng.range(0, 1_000_000));
        let (rto, mut reference_rto) = random_rto(&mut rng);
        let mut actual = random_timer(&mut rng, now);
        actual.rtt = rto;
        let mut expected = actual.clone();
        let actual_outcome = tcp_timer_rexmt(&mut actual, now);
        let expected_outcome = ref_rexmt(&mut expected, &mut reference_rto, now);
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

#[test]
fn randomized_max_unacked_check_matches_reference_for_10k_operations() {
    let mut rng = ModelRng::new(0x309_0005);
    for step in 0..10_000 {
        let now = u64::from(rng.range(0, 1_000_000));
        let mut tp = random_timer(&mut rng, now);
        tp.acktime_ms = if rng.below(3) == 0 {
            0
        } else {
            now.saturating_sub(u64::from(rng.range(0, 100_000)))
        };
        tp.maxunacktime_ms = if rng.below(3) == 0 {
            0
        } else {
            u64::from(rng.range(1, 200_000))
        };
        assert_eq!(
            tp.maxunacktime_exceeded(now),
            ref_maxunack(&tp, now),
            "maxunack step {step}"
        );
    }
}
