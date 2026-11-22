// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD at donor revision 36eddebcc36cb0775aa2962005b9c05419a6ff01:
//   sys/netinet/tcp_timer.c   tcp_timer_rexmt(), tcp_timer_keep(), tcp_timer_2msl(),
//                             tcp_backoff[], tcp_totbackoff
//   sys/netinet/tcp_timer.h   TCPTV_MSL, TCPTV_KEEP_INIT, TCPTV_KEEP_IDLE,
//                             TCPTV_KEEPINTVL, TCPTV_KEEPCNT, TCPTV_MIN,
//                             TCPTV_REXMTMAX, TCP_MAXRXTSHIFT, TCPT_RANGESET()
//   sys/netinet/tcp_var.h     TCP_REXMTVAL(), TCP_RTT_SHIFT, TCP_RTTVAR_SHIFT
//   sys/netinet/tcp_subr.c    initial t_srtt/t_rttvar seed, tcp_rttdflt, timer defaults
//
// Copyright (C) 1995, 1996, 1997, and 1998 WIDE Project.
// All rights reserved.
//
// Copyright (c) 1997, 1998, 2001, 2005 The NetBSD Foundation, Inc.
// All rights reserved.
//
// Copyright (c) 1982, 1986, 1988, 1990, 1993, 1995
//	The Regents of the University of California.  All rights reserved.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
// 3. Neither the name of the University nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE REGENTS AND CONTRIBUTORS ``AS IS'' AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE
// IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE
// ARE DISCLAIMED.  IN NO EVENT SHALL THE REGENTS OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS
// OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION)
// HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
// LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY
// OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF
// SUCH DAMAGE.

//! NetBSD TCP timer state machine: retransmit, keepalive and 2MSL.
//!
//! `TimerState` carries the timer-visible half of a NetBSD `tcpcb` (`sys/netinet/tcp_var.h`) and
//! the three timer entry points return the action the caller must perform. The state is
//! deliberately separate from `tcp_state.rs`: K265/Jerry owns the live callout entry and maps the
//! outcomes onto `TcpConnection`, congestion control and `tcp_output`.
//!
//! Time is in milliseconds and follows the stack convention (`Instant` is an `i64` count).
//! NetBSD stores the same values in slow ticks (`PR_SLOWHZ`); the `_MS` constants are the
//! `PR_SLOWHZ = 2` values converted once (one slow tick = 500 ms).

// K265/Jerry owns the live callout entry; until that lands this module has no non-test caller.
#![allow(dead_code)]

use crate::rto::{
    RtoEstimator, TCP_MAXRXTSHIFT, TCPTV_MIN_MS, TCPTV_REXMTMAX_MS, TCPTV_RTOBASE_MS,
};
use crate::{Errno, TcpState};

/// NetBSD `TCPTV_MSL`: maximum segment lifetime (30 s).
pub const TCPTV_MSL_MS: u64 = 30_000;

/// NetBSD `TCPTV_KEEP_INIT`: time to establish a connection (75 s).
pub const TCPTV_KEEP_INIT_MS: u64 = 75_000;

/// NetBSD `TCPTV_KEEP_IDLE`: idle time before keepalive probes (2 h).
pub const TCPTV_KEEP_IDLE_MS: u64 = 120 * 60 * 1_000;

/// NetBSD `TCPTV_KEEPINTVL`: interval between keepalive probes (75 s).
pub const TCPTV_KEEPINTVL_MS: u64 = 75_000;

/// NetBSD `TCPTV_KEEPCNT`: probes before the keepalive drop (8).
pub const TCPTV_KEEPCNT: u64 = 8;

/// The timer-visible fields of a NetBSD `tcpcb`.
///
/// Fields map one-to-one to `sys/netinet/tcp_var.h`; the suffix `_ms` marks a Raven millisecond
/// value where NetBSD stores slow ticks.
#[derive(Clone, Debug)]
pub struct TimerState {
    /// `t_state`.
    pub state: TcpState,
    /// `TF_DEAD`/closed: the connection was dropped or closed and every callout is a no-op.
    pub closed: bool,
    /// `SO_KEEPALIVE`.
    pub keepalive: bool,
    /// Number of SACK scoreboard entries; `tcp_free_sackholes` clears them.
    pub sackholes: usize,
    /// `snd_una`.
    pub snd_una: u32,
    /// `snd_nxt`.
    pub snd_nxt: u32,
    /// `snd_max`.
    pub snd_max: u32,
    /// `snd_high`.
    pub snd_high: u32,
    /// `snd_fack`.
    pub snd_fack: u32,
    /// `snd_recover`.
    pub snd_recover: u32,
    /// `rcv_nxt`.
    pub rcv_nxt: u32,
    /// `t_rcvtime` in milliseconds: when the last segment arrived.
    pub rcvtime_ms: u64,
    /// `t_srtt`/`t_rttvar` plus the `TCP_REXMTVAL` base in the RFC 6298 estimator.
    pub rtt: RtoEstimator,
    /// `t_rttmin` in milliseconds.
    pub rttmin_ms: u64,
    /// `t_rxtshift`.
    pub rxtshift: u8,
    /// `t_rxtcur` in milliseconds.
    pub rxtcur_ms: u64,
    /// `t_rtttime != 0`: a segment is being timed.
    pub rtttime_active: bool,
    /// `t_mtudisc`: path MTU discovery is enabled for this connection.
    pub mtudisc: bool,
    /// `t_softerror`: the non-fatal error saved for the next retransmit drop.
    pub soft_error: Option<Errno>,
    /// `TF_SYN_REXMT`: the timer retransmitted a SYN in `SYN_SENT`.
    pub syn_rexmt: bool,
    /// `t_keepinit` in milliseconds.
    pub keepinit_ms: u64,
    /// `t_keepidle` in milliseconds.
    pub keepidle_ms: u64,
    /// `t_keepintvl` in milliseconds.
    pub keepintvl_ms: u64,
    /// `t_keepcnt`: keepalive probes, at least 1.
    pub keepcnt: u64,
    /// The armed `TCPT_REXMT` callout, in milliseconds.
    pub rexmt_deadline_ms: Option<u64>,
    /// The armed `TCPT_KEEP` callout, in milliseconds.
    pub keep_deadline_ms: Option<u64>,
    /// The armed `TCPT_2MSL` callout, in milliseconds.
    pub two_msl_deadline_ms: Option<u64>,
}

impl TimerState {
    /// Create the timer half of a `tcpcb` with the NetBSD `tcp_tcpcb_template()` defaults.
    #[must_use]
    pub fn new(state: TcpState, now_ms: u64) -> Self {
        Self {
            state,
            closed: false,
            keepalive: false,
            sackholes: 0,
            snd_una: 0,
            snd_nxt: 0,
            snd_max: 0,
            snd_high: 0,
            snd_fack: 0,
            snd_recover: 0,
            rcv_nxt: 0,
            rcvtime_ms: now_ms,
            rtt: RtoEstimator::new(TCPTV_RTOBASE_MS, TCPTV_MIN_MS, TCPTV_REXMTMAX_MS, 1),
            rttmin_ms: TCPTV_MIN_MS,
            rxtshift: 0,
            rxtcur_ms: TCPTV_RTOBASE_MS,
            rtttime_active: false,
            mtudisc: true,
            soft_error: None,
            syn_rexmt: false,
            keepinit_ms: TCPTV_KEEP_INIT_MS,
            keepidle_ms: TCPTV_KEEP_IDLE_MS,
            keepintvl_ms: TCPTV_KEEPINTVL_MS,
            keepcnt: TCPTV_KEEPCNT,
            rexmt_deadline_ms: None,
            keep_deadline_ms: None,
            two_msl_deadline_ms: None,
        }
    }

    /// `TP_KEEPINIT(tp)`: the configured value or the NetBSD default.
    #[must_use]
    pub fn effective_keepinit_ms(&self) -> u64 {
        if self.keepinit_ms == 0 {
            TCPTV_KEEP_INIT_MS
        } else {
            self.keepinit_ms
        }
    }

    /// `TP_KEEPIDLE(tp)`: the configured value or the NetBSD default.
    #[must_use]
    pub fn effective_keepidle_ms(&self) -> u64 {
        if self.keepidle_ms == 0 {
            TCPTV_KEEP_IDLE_MS
        } else {
            self.keepidle_ms
        }
    }

    /// `TP_KEEPINTVL(tp)`: the configured value or the NetBSD default.
    #[must_use]
    pub fn effective_keepintvl_ms(&self) -> u64 {
        if self.keepintvl_ms == 0 {
            TCPTV_KEEPINTVL_MS
        } else {
            self.keepintvl_ms
        }
    }

    /// `t_maxidle`: `t_keepcnt * t_keepintvl`, with `t_keepcnt >= 1`.
    #[must_use]
    pub fn maxidle_ms(&self) -> u64 {
        self.keepcnt
            .max(1)
            .saturating_mul(self.effective_keepintvl_ms())
    }

    /// Arm `TCPT_KEEP` for the connect path: `TP_KEEPINIT` in SYN states.
    pub fn arm_keep_init(&mut self, now_ms: u64) {
        self.keep_deadline_ms = Some(now_ms.saturating_add(self.effective_keepinit_ms()));
    }

    /// Arm `TCPT_2MSL` the way `tcp_twstart()` does: `2 * MSL`.
    pub fn arm_two_msl(&mut self, now_ms: u64, msl_ms: u64) {
        self.two_msl_deadline_ms = Some(now_ms.saturating_add(msl_ms.saturating_mul(2)));
    }
}

/// The result of NetBSD `tcp_timer_rexmt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RexmtOutcome {
    /// What the caller must do next.
    pub action: RexmtAction,
}

/// The action requested by NetBSD `tcp_timer_rexmt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RexmtAction {
    /// The callout had not expired, or the connection was already closed.
    Ignored,
    /// Retransmit from `snd_una`; `TCPT_REXMT` is rearmed to `t_rxtcur`.
    Retransmit {
        /// The new `t_rxtcur` in milliseconds.
        rxtcur_ms: u64,
        /// The retransmit is a `SYN_SENT` retransmit (`TF_SYN_REXMT`).
        syn: bool,
        /// `tcp_free_sackholes()` ran before the retransmit.
        free_sackholes: bool,
        /// `t_mtudisc` was turned off after `TCP_MAXRXTSHIFT / 6` losses.
        mtudisc_disabled: bool,
        /// `srtt` was folded into `rttvar` after `TCP_MAXRXTSHIFT / 4` losses.
        losing: bool,
    },
    /// More than `TCP_MAXRXTSHIFT` retransmits: drop with `t_softerror` or `ETIMEDOUT`.
    Drop {
        /// `t_softerror` when set, otherwise `ETIMEDOUT`.
        error: Errno,
    },
}

/// The result of NetBSD `tcp_timer_keep`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeepOutcome {
    /// The callout had not expired, or the connection was already closed.
    Ignored,
    /// Send the zero-length probe with `seq = snd_una - 1` and `ack = rcv_nxt`; `TCPT_KEEP` is
    /// rearmed to `TP_KEEPINTVL`.
    Probe {
        /// `snd_una - 1`, deliberately outside the peer receive window.
        seq: u32,
        /// `rcv_nxt`.
        ack: u32,
    },
    /// Keepalive is off or the state is past `CLOSE_WAIT`: `TCPT_KEEP` is rearmed to `TP_KEEPIDLE`.
    Rearm,
    /// The connection was dropped.
    Drop {
        /// Always `ETIMEDOUT` in NetBSD `tcp_timer_keep`.
        error: Errno,
    },
}

/// The result of NetBSD `tcp_timer_2msl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TwoMslOutcome {
    /// The callout had not expired, or the connection was already closed.
    Ignored,
    /// Idle inside `t_maxidle`: `TCPT_2MSL` is rearmed to `TP_KEEPINTVL`.
    Rearm,
    /// TIME-WAIT or idle past `t_maxidle`: close the connection.
    Close,
}

/// NetBSD `tcp_timer_rexmt` (`sys/netinet/tcp_timer.c`).
///
/// The caller performs the retransmit (`tcp_output`), the congestion-control
/// `slow_retransmit`, the SACK-hole clear/FACK reset and the PMTUD fallback.
#[must_use]
pub fn tcp_timer_rexmt(tp: &mut TimerState, now_ms: u64) -> RexmtOutcome {
    if tp.closed || !callout_expired(tp.rexmt_deadline_ms, now_ms) {
        return RexmtOutcome {
            action: RexmtAction::Ignored,
        };
    }

    // tcp_free_sackholes(tp); tp->snd_fack = tp->snd_una;
    tp.sackholes = 0;
    tp.snd_fack = tp.snd_una;

    tp.rxtshift = tp.rxtshift.saturating_add(1);
    if tp.rxtshift > TCP_MAXRXTSHIFT {
        tp.rxtshift = TCP_MAXRXTSHIFT;
        let error = tp.soft_error.take().unwrap_or(Errno::ETIMEDOUT);
        drop_state(tp);
        return RexmtOutcome {
            action: RexmtAction::Drop { error },
        };
    }

    let mut rto = tp.rtt.rexmtval_ms();
    if rto < tp.rttmin_ms {
        rto = tp.rttmin_ms;
    }
    tp.rxtcur_ms = RtoEstimator::backoff_from_ms(rto, tp.rxtshift, tp.rttmin_ms, TCPTV_REXMTMAX_MS);
    tp.rexmt_deadline_ms = Some(now_ms.saturating_add(tp.rxtcur_ms));

    // t_mtudisc fallback after TCP_MAXRXTSHIFT / 6 losses.
    let mtudisc_disabled = tp.mtudisc && tp.rxtshift > TCP_MAXRXTSHIFT / 6;
    if mtudisc_disabled {
        tp.mtudisc = false;
    }

    // Losing path: TCP_MAXRXTSHIFT / 4, fold srtt into rttvar and clear srtt.
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

/// NetBSD `tcp_timer_keep` (`sys/netinet/tcp_timer.c`).
#[must_use]
pub fn tcp_timer_keep(tp: &mut TimerState, now_ms: u64) -> KeepOutcome {
    if tp.closed || !callout_expired(tp.keep_deadline_ms, now_ms) {
        return KeepOutcome::Ignored;
    }

    if !established(tp.state) {
        drop_state(tp);
        return KeepOutcome::Drop {
            error: Errno::ETIMEDOUT,
        };
    }

    if tp.keepalive && keep_state_allows_probe(tp.state) {
        let maxidle = tp.maxidle_ms();
        if maxidle > 0
            && now_ms.saturating_sub(tp.rcvtime_ms)
                >= tp.effective_keepidle_ms().saturating_add(maxidle)
        {
            drop_state(tp);
            return KeepOutcome::Drop {
                error: Errno::ETIMEDOUT,
            };
        }
        tp.keep_deadline_ms = Some(now_ms.saturating_add(tp.effective_keepintvl_ms()));
        return KeepOutcome::Probe {
            seq: tp.snd_una.wrapping_sub(1),
            ack: tp.rcv_nxt,
        };
    }

    tp.keep_deadline_ms = Some(now_ms.saturating_add(tp.effective_keepidle_ms()));
    KeepOutcome::Rearm
}

/// NetBSD `tcp_timer_2msl` (`sys/netinet/tcp_timer.c`).
#[must_use]
pub fn tcp_timer_2msl(tp: &mut TimerState, now_ms: u64) -> TwoMslOutcome {
    if tp.closed || !callout_expired(tp.two_msl_deadline_ms, now_ms) {
        return TwoMslOutcome::Ignored;
    }

    // tcp_free_sackholes(tp); tp->snd_fack = tp->snd_una;
    tp.sackholes = 0;
    tp.snd_fack = tp.snd_una;

    let maxidle = tp.maxidle_ms();
    if tp.state != TcpState::TimeWait
        && (maxidle == 0 || now_ms.saturating_sub(tp.rcvtime_ms) <= maxidle)
    {
        tp.two_msl_deadline_ms = Some(now_ms.saturating_add(tp.effective_keepintvl_ms()));
        TwoMslOutcome::Rearm
    } else {
        drop_state(tp);
        TwoMslOutcome::Close
    }
}

/// NetBSD `TCPS_HAVEESTABLISHED`: the state is ESTABLISHED or later.
#[must_use]
pub const fn established(state: TcpState) -> bool {
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

/// NetBSD `tcp_timer_keep`'s `t_state <= TCPS_CLOSE_WAIT`.
///
/// NetBSD numbers `TCPS_ESTABLISHED = 4` and `TCPS_CLOSE_WAIT = 5`, so only those two states take
/// the probe branch; every later state (FIN_WAIT_1, CLOSING, LAST_ACK, FIN_WAIT_2, TIME_WAIT)
/// re-arms to `TP_KEEPIDLE` instead.
#[must_use]
pub const fn keep_state_allows_probe(state: TcpState) -> bool {
    matches!(state, TcpState::Established | TcpState::CloseWait)
}

/// NetBSD `callout_expired()`: an armed deadline at or before `now`.
#[must_use]
pub const fn callout_expired(deadline_ms: Option<u64>, now_ms: u64) -> bool {
    matches!(deadline_ms, Some(deadline) if now_ms >= deadline)
}

fn drop_state(tp: &mut TimerState) {
    tp.closed = true;
    tp.rexmt_deadline_ms = None;
    tp.keep_deadline_ms = None;
    tp.two_msl_deadline_ms = None;
}

#[cfg(test)]
#[path = "tcp_timer_ut.rs"]
mod tests;
