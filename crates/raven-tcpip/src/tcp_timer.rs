// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from NetBSD at donor revision 36eddebcc36cb0775aa2962005b9c05419a6ff01:
//   sys/netinet/tcp_timer.c   tcp_timer_rexmt(), tcp_timer_keep(), tcp_timer_2msl(),
//                             tcp_backoff[], tcp_totbackoff
//   sys/netinet/tcp_timer.h   TCPTV_* constants, TCP_MAXRXTSHIFT, TCPT_RANGESET
//   sys/netinet/tcp_var.h     TCP_REXMTVAL(), TCP_RTT_SHIFT, TCP_RTTVAR_SHIFT,
//                             t_keepidle/t_keepintvl/t_keepcnt/t_maxidle
//   sys/netinet/tcp_subr.c    tcp_newtcpcb() timer defaults
//   sys/netinet/tcp_timewait.c tcp_twstart(): TT_2MSL = 2 * tcp_eff_msl(tp)
//
// Copyright (c) 1997, 1998, 2001, 2005 The NetBSD Foundation, Inc.
// All rights reserved.
//
// This code is derived from software contributed to The NetBSD Foundation
// by Jason R. Thorpe and Kevin M. Lahey of the Numerical Aerospace Simulation
// Facility, NASA Ames Research Center, and by Charles M. Hannum.
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions
// are met:
// 1. Redistributions of source code must retain the above copyright
//    notice, this list of conditions and the following disclaimer.
// 2. Redistributions in binary form must reproduce the above copyright
//    notice, this list of conditions and the following disclaimer in the
//    documentation and/or other materials provided with the distribution.
//
// THIS SOFTWARE IS PROVIDED BY THE NETBSD FOUNDATION, INC. AND CONTRIBUTORS
// ``AS IS'' AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED
// TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR
// PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE FOUNDATION OR CONTRIBUTORS
// BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR
// CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF
// SUBSTITUTE GOODS OR SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS
// INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN
// CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE)
// ARISING IN ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
// POSSIBILITY OF SUCH DAMAGE.
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
//! `TimerState` carries the timer-visible half of a NetBSD `tcpcb`
//! (`sys/netinet/tcp_var.h`) and the three timer entries return the action the caller performs.
//! The state is deliberately separate from `tcp_state.rs`: K265/Jerry owns the live callout entry
//! and maps the outcomes onto `TcpConnection`, congestion Control and `tcp_output`. Persist is
//! K254/Jerry and is not ported here.
//!
//! Time is in milliseconds and follows the stack convention (`Instant` is an `i64` count).
//! NetBSD keeps the values in slow ticks; the `_MS` constants convert once (1 slow tick = 500 ms).

// K265/Jerry owns the live callout entry; until that lands this module has no non-test caller.
#![allow(dead_code)]

use crate::rto::{
    TCP_MAXRXTSHIFT, TCP_RTT_INVALIDATE, TCPTV_MIN_MS, TCPTV_REXMTMAX_MS, TCPTV_RTOBASE_MS,
};
use crate::{Errno, RtoEstimator, TcpState};

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

/// NetBSD `tcp_maxpersistidle` default and `t_maxidle`: `keepcnt * keepintvl`.
pub const TCPTV_MAXIDLE_MS: u64 = TCPTV_KEEPCNT * TCPTV_KEEPINTVL_MS;

/// NetBSD `TCP_MAXRXTSHIFT / 6`: the PMTUD black-hole shift.
pub const PMTUD_BLACKHOLE_SHIFT: u8 = TCP_MAXRXTSHIFT / 6;

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
    /// `snd_high` (`tcp_timer_rexmt` saves `snd_max` here).
    pub snd_high: u32,
    /// `rcv_nxt`.
    pub rcv_nxt: u32,
    /// `t_rcvtime` in milliseconds: when the last segment arrived.
    pub rcvtime_ms: u64,
    /// `t_srtt`/`t_rttvar` plus the `TCP_REXMTVAL` base in the RFC 2988 estimator.
    pub rtt: RtoEstimator,
    /// `t_rttmin` in milliseconds.
    pub rttmin_ms: u64,
    /// `tcp_rexmit_max`: the maximum armed RTO.
    pub rexmit_max_ms: u64,
    /// `V_tcp_retries`/`TCP_MAXRXTSHIFT`: retransmits before the drop.
    pub retries: u8,
    /// `t_rxtshift`.
    pub rxtshift: u8,
    /// `t_rxtcur` in milliseconds.
    pub rxtcur_ms: u64,
    /// `t_rtttime != 0`: a segment is being timed.
    pub rtttime_active: bool,
    /// `t_keepinit` in milliseconds.
    pub keepinit_ms: u64,
    /// `t_keepidle` in milliseconds.
    pub keepidle_ms: u64,
    /// `t_keepintvl` in milliseconds.
    pub keepintvl_ms: u64,
    /// `t_keepcnt`.
    pub keepcnt: u64,
    /// `t_maxidle` in milliseconds; 0 means no idle limit.
    pub maxidle_ms: u64,
    /// `t_mtudisc`: path-MTU discovery is active.
    pub mtudisc: bool,
    /// `TF_SYN_REXMT`: the last retransmit was a SYN.
    pub syn_rexmt: bool,
    /// `t_snd_fack`: SACK forward-ack estimate, reset on retransmit.
    pub snd_fack: u32,
    /// The armed `TT_REXMT` callout, in milliseconds.
    pub rexmt_deadline_ms: Option<u64>,
    /// The armed `TT_KEEP` callout, in milliseconds.
    pub keep_deadline_ms: Option<u64>,
    /// The armed `TT_2MSL` callout, in milliseconds.
    pub two_msl_deadline_ms: Option<u64>,
}

impl TimerState {
    /// Create the timer half of a `tcpcb` with the NetBSD `tcp_newtcpcb()`/`tcp_timer_init()`
    /// defaults.
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
            rcv_nxt: 0,
            rcvtime_ms: now_ms,
            rtt: RtoEstimator::new(TCPTV_RTOBASE_MS, TCPTV_MIN_MS, TCPTV_REXMTMAX_MS, 1),
            rttmin_ms: TCPTV_MIN_MS,
            rexmit_max_ms: TCPTV_REXMTMAX_MS,
            retries: TCP_MAXRXTSHIFT,
            rxtshift: 0,
            rxtcur_ms: TCPTV_RTOBASE_MS,
            rtttime_active: false,
            keepinit_ms: TCPTV_KEEP_INIT_MS,
            keepidle_ms: TCPTV_KEEP_IDLE_MS,
            keepintvl_ms: TCPTV_KEEPINTVL_MS,
            keepcnt: TCPTV_KEEPCNT,
            maxidle_ms: TCPTV_MAXIDLE_MS,
            mtudisc: true,
            syn_rexmt: false,
            snd_fack: 0,
            rexmt_deadline_ms: None,
            keep_deadline_ms: None,
            two_msl_deadline_ms: None,
        }
    }

    /// Arm `TT_KEEP` for the connect path (`tcp_subr.c`: `TP_KEEPINIT(tp)` on SYN states).
    pub fn arm_keep_init(&mut self, now_ms: u64) {
        self.keep_deadline_ms = Some(now_ms.saturating_add(self.keepinit_ms));
    }

    /// Arm `TT_2MSL` the way `tcp_twstart()` does: `2 * MSL`.
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
    /// Retransmit from `snd_una`; the caller runs `slow_retransmit`, arms output and sends.
    Retransmit {
        /// The new `t_rxtcur` in milliseconds.
        rxtcur_ms: u64,
        /// The retransmit is a SYN (`t_state == TCPS_SYN_SENT`); `TF_SYN_REXMT` is set.
        syn: bool,
        /// `t_mtudisc` was turned off after the black-hole shift.
        pmtud_blackhole: bool,
        /// The srtt was moved into rttvar and zeroed (`inpcb_losing`).
        losing_reset: bool,
    },
    /// More than `V_tcp_retries` retransmits: drop.
    Drop {
        /// Always `ETIMEDOUT` in NetBSD's timer path unless `t_softerror` is set.
        error: Errno,
    },
}

/// The result of NetBSD `tcp_timer_keep`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeepOutcome {
    /// The callout had not expired, or the connection was already closed.
    Ignored,
    /// Send a zero-length probe with `seq = snd_una - 1` and `ack = rcv_nxt`; `TT_KEEP` is
    /// rearmed to `TP_KEEPINTVL`.
    Probe {
        /// `snd_una - 1`, deliberately outside the peer receive window.
        seq: u32,
        /// `rcv_nxt`.
        ack: u32,
    },
    /// Keepalive is off (or the state is past CLOSE_WAIT): `TT_KEEP` is rearmed to `TP_KEEPIDLE`.
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
    /// Idle inside `TP_MAXIDLE`: `TT_2MSL` is rearmed to `TP_KEEPINTVL`.
    Rearm,
    /// TIME-WAIT or idle past `TP_MAXIDLE`: close the connection.
    Close,
}

/// NetBSD `tcp_timer_rexmt` (`sys/netinet/tcp_timer.c`).
///
/// The caller performs the retransmit (`tcp_output`), the congestion-control response
/// (`slow_retransmit`) and the PLPMTUD action. The function itself clears the SACK scoreboard,
/// resets `snd_fack`, applies the backoff, the PMTUD black-hole decision and the losing reset.
#[must_use]
pub fn tcp_timer_rexmt(tp: &mut TimerState, now_ms: u64) -> RexmtOutcome {
    if tp.closed || !callout_expired(tp.rexmt_deadline_ms, now_ms) {
        return RexmtOutcome {
            action: RexmtAction::Ignored,
        };
    }

    // NetBSD: tcp_free_sackholes(tp); tp->snd_fack = tp->snd_una;
    tp.sackholes = 0;
    tp.snd_fack = tp.snd_una;

    tp.rxtshift = tp.rxtshift.saturating_add(1);
    if tp.rxtshift > tp.retries {
        tp.rxtshift = tp.retries;
        drop_state(tp);
        return RexmtOutcome {
            action: RexmtAction::Drop {
                error: Errno::ETIMEDOUT,
            },
        };
    }

    // NetBSD: rto = TCP_REXMTVAL(tp); if (rto < t_rttmin) rto = t_rttmin;
    // TCPT_RANGESET(t_rxtcur, rto * tcp_backoff[t_rxtshift], t_rttmin, TCPTV_REXMTMAX).
    // TCP_REXMTVAL is 0 before the first sample, so the first arm is rttmin * backoff[1].
    let base_ms = if tp.rtt.has_sample() {
        tp.rtt.rexmtval_ms()
    } else {
        0
    };
    tp.rxtcur_ms = RtoEstimator::backoff(base_ms, tp.rxtshift, tp.rttmin_ms, tp.rexmit_max_ms);
    tp.rexmt_deadline_ms = Some(now_ms.saturating_add(tp.rxtcur_ms));

    let pmtud_blackhole = tp.mtudisc && tp.rxtshift > PMTUD_BLACKHOLE_SHIFT;
    if pmtud_blackhole {
        tp.mtudisc = false;
    }

    // NetBSD: if (t_rxtshift > TCP_MAXRXTSHIFT / 4) { inpcb_losing;
    // t_rttvar += t_srtt >> TCP_RTT_SHIFT; t_srtt = 0; }
    let losing_reset = tp.rxtshift > TCP_RTT_INVALIDATE;
    if losing_reset {
        tp.rtt.losing_reset();
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
            pmtud_blackhole,
            losing_reset,
        },
    }
}

/// NetBSD `tcp_timer_keep` (`sys/netinet/tcp_timer.c`).
#[must_use]
pub fn tcp_timer_keep(tp: &mut TimerState, now_ms: u64) -> KeepOutcome {
    if tp.closed || !callout_expired(tp.keep_deadline_ms, now_ms) {
        return KeepOutcome::Ignored;
    }

    // NetBSD: if (TCPS_HAVEESTABLISHED(tp->t_state) == 0) goto dropit;
    if !established(tp.state) {
        drop_state(tp);
        return KeepOutcome::Drop {
            error: Errno::ETIMEDOUT,
        };
    }

    // NetBSD: if (so->so_options & SO_KEEPALIVE && tp->t_state <= TCPS_CLOSE_WAIT) { ... }
    if tp.keepalive && keep_probe_allowed(tp.state) {
        if tp.maxidle_ms > 0
            && now_ms.saturating_sub(tp.rcvtime_ms) >= tp.keepidle_ms.saturating_add(tp.maxidle_ms)
        {
            drop_state(tp);
            return KeepOutcome::Drop {
                error: Errno::ETIMEDOUT,
            };
        }
        tp.keep_deadline_ms = Some(now_ms.saturating_add(tp.keepintvl_ms));
        return KeepOutcome::Probe {
            seq: tp.snd_una.wrapping_sub(1),
            ack: tp.rcv_nxt,
        };
    }

    tp.keep_deadline_ms = Some(now_ms.saturating_add(tp.keepidle_ms));
    KeepOutcome::Rearm
}

/// NetBSD `tcp_timer_2msl` (`sys/netinet/tcp_timer.c`).
#[must_use]
pub fn tcp_timer_2msl(tp: &mut TimerState, now_ms: u64) -> TwoMslOutcome {
    if tp.closed || !callout_expired(tp.two_msl_deadline_ms, now_ms) {
        return TwoMslOutcome::Ignored;
    }

    // NetBSD: tcp_free_sackholes(tp); tp->snd_fack = tp->snd_una;
    tp.sackholes = 0;
    tp.snd_fack = tp.snd_una;

    // NetBSD: if (t_state != TIME_WAIT && (t_maxidle == 0 || now - t_rcvtime <= t_maxidle))
    // TCP_TIMER_ARM(TCPT_2MSL, t_keepintvl); else tcp_close(tp).
    let idle = now_ms.saturating_sub(tp.rcvtime_ms);
    if tp.state != TcpState::TimeWait && (tp.maxidle_ms == 0 || idle <= tp.maxidle_ms) {
        tp.two_msl_deadline_ms = Some(now_ms.saturating_add(tp.keepintvl_ms));
        TwoMslOutcome::Rearm
    } else {
        drop_state(tp);
        TwoMslOutcome::Close
    }
}

/// NetBSD `TCPS_HAVEESTABLISHED`: ESTABLISHED or later.
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
#[must_use]
pub const fn keep_probe_allowed(state: TcpState) -> bool {
    matches!(
        state,
        TcpState::Established | TcpState::FinWait1 | TcpState::FinWait2 | TcpState::CloseWait
    )
}

/// `callout_expired()`: an armed deadline at or before `now`.
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
