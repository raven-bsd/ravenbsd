// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD at donor revision 133344405f532bd54ff3629a77d4c4e062f01fdb:
//   sys/netinet/tcp_timer.c   tcp_timer_rexmt(), tcp_timer_keep(), tcp_timer_2msl(),
//                             tcp_backoff[], tcp_totbackoff, tcp_maxunacktime_check()
//   sys/netinet/tcp_timer.h   TCPTV_* constants, TCP_MAXRXTSHIFT, TCP_RTT_INVALIDATE,
//                             TCPT_RANGESET(), TP_KEEPIDLE/TP_KEEPINTVL/TP_MAXIDLE
//   sys/netinet/tcp_var.h     TCP_REXMTVAL(), TCP_RTT_SHIFT, TCP_RTTVAR_SHIFT, TCP_DELTA_SHIFT
//   sys/netinet/tcp_subr.c    initial t_srtt/t_rttvar seed, tcp_rexmit_* defaults
//   sys/netinet/tcp_usrreq.c  keepalive option arming (TCP_KEEPIDLE/KEEPINTVL/KEEPCNT)
//   sys/netinet/tcp_timewait.c tcp_twstart(): TT_2MSL = 2 * tcp_eff_msl(tp)
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

//! FreeBSD TCP timer state machine: retransmit, keepalive and 2MSL.
//!
//! `TimerState` carries the timer-visible half of a FreeBSD `tcpcb` (`sys/netinet/tcp_var.h`)
//! and the three timer entry points return the action the caller must perform. The state is
//! deliberately separate from `tcp_state.rs`: K265/Jerry owns the live callout entry and maps
//! the outcomes onto `TcpConnection`, congestion control and `tcp_output`.
//!
//! Time is in milliseconds and follows the stack convention (`Instant` is an `i64` count).
//! FreeBSD keeps the same values in `int` ticks; the `_MS` constants are the `MSEC_2_TICKS`
//! values converted once.

// K265/Jerry owns the live callout entry; until that lands this module has no non-test caller.
#![allow(dead_code)]

use crate::rto::{
    TCP_MAXRXTSHIFT, TCP_RTT_INVALIDATE, TCPTV_CPU_VAR_MS, TCPTV_MIN_MS, TCPTV_REXMTMAX_MS,
    TCPTV_RTOBASE_MS,
};
use crate::{Errno, RtoEstimator, TcpState};

/// FreeBSD `TCPTV_MSL`: maximum segment lifetime (30 s).
pub const TCPTV_MSL_MS: u64 = 30_000;

/// FreeBSD `TCPTV_KEEP_INIT`: time to establish a connection (75 s).
pub const TCPTV_KEEP_INIT_MS: u64 = 75_000;

/// FreeBSD `TCPTV_KEEP_IDLE`: idle time before keepalive probes (2 h).
pub const TCPTV_KEEP_IDLE_MS: u64 = 120 * 60 * 1_000;

/// FreeBSD `TCPTV_KEEPINTVL`: interval between keepalive probes (75 s).
pub const TCPTV_KEEPINTVL_MS: u64 = 75_000;

/// FreeBSD `TCPTV_KEEPCNT`: probes before the keepalive drop (8).
pub const TCPTV_KEEPCNT: u64 = 8;

/// FreeBSD `TCPTV_FINWAIT2_TIMEOUT`: fast FIN_WAIT_2 recycle timeout (60 s).
pub const TCPTV_FINWAIT2_TIMEOUT_MS: u64 = 60_000;

/// The timer-visible fields of a FreeBSD `tcpcb`.
///
/// Fields map one-to-one to `sys/netinet/tcp_var.h`; the suffix `_ms` marks a Raven millisecond
/// value where FreeBSD stores ticks.
#[derive(Clone, Debug)]
pub struct TimerState {
    /// `t_state`.
    pub state: TcpState,
    /// `TF_DEAD`/closed: the connection was dropped or closed and every callout is a no-op.
    pub closed: bool,
    /// `V_tcp_always_keepalive`: keepalive applies even without `SO_KEEPALIVE`.
    pub always_keepalive: bool,
    /// `SO_KEEPALIVE`.
    pub keepalive: bool,
    /// `SBS_CANTRCVMORE` on the receive socket buffer.
    pub receiver_closed: bool,
    /// `tcp_fast_finwait2_recycle`.
    pub fast_finwait2_recycle: bool,
    /// Number of SACK scoreboard entries; `tcp_free_sackholes` clears them.
    pub sackholes: usize,
    /// `snd_una`.
    pub snd_una: u32,
    /// `snd_nxt`.
    pub snd_nxt: u32,
    /// `snd_max`.
    pub snd_max: u32,
    /// `snd_recover`.
    pub snd_recover: u32,
    /// `rcv_nxt`.
    pub rcv_nxt: u32,
    /// `t_acktime` in milliseconds: when a sufficiently large ACK last arrived, 0 if unset.
    pub acktime_ms: u64,
    /// `t_rcvtime` in milliseconds: when the last segment arrived.
    pub rcvtime_ms: u64,
    /// `t_srtt`/`t_rttvar` plus the `TCP_REXMTVAL` base in the RFC 6298 estimator.
    pub rtt: RtoEstimator,
    /// `t_rttmin` in milliseconds.
    pub rttmin_ms: u64,
    /// `tcp_rexmit_slop`: the `TCPT_RANGESET` processing slop in milliseconds.
    pub rexmit_slop_ms: u64,
    /// `tcp_rexmit_initial`: the RTO used for SYN retransmits.
    pub rexmit_initial_ms: u64,
    /// `tcp_rexmit_max`: the maximum armed RTO.
    pub rexmit_max_ms: u64,
    /// `V_tcp_retries`: retransmits before the drop.
    pub retries: u8,
    /// `tcp_rexmit_drop_options`: drop RFC 1323/SACK options on the third SYN retransmit.
    pub rexmit_drop_options: bool,
    /// `tcp_maxunacktime`: max time without progress, 0 disables the check.
    pub maxunacktime_ms: u64,
    /// `t_rxtshift`.
    pub rxtshift: u8,
    /// `t_rxtcur` in milliseconds.
    pub rxtcur_ms: u64,
    /// `t_rtttime != 0`: a segment is being timed.
    pub rtttime_active: bool,
    /// `TF_ACKNOW`: force an ACK on the next output.
    pub acknow: bool,
    /// `t_keepinit` in milliseconds.
    pub keepinit_ms: u64,
    /// `t_keepidle` in milliseconds.
    pub keepidle_ms: u64,
    /// `t_keepintvl` in milliseconds.
    pub keepintvl_ms: u64,
    /// `t_keepcnt`: keepalive probes, 0 selects the global default.
    pub keepcnt: u64,
    /// The armed `TT_REXMT` callout, in milliseconds.
    pub rexmt_deadline_ms: Option<u64>,
    /// The armed `TT_KEEP` callout, in milliseconds.
    pub keep_deadline_ms: Option<u64>,
    /// The armed `TT_2MSL` callout, in milliseconds.
    pub two_msl_deadline_ms: Option<u64>,
}

impl TimerState {
    /// Create the timer half of a `tcpcb` with the FreeBSD defaults from `tcp_newtcpcb()`.
    #[must_use]
    pub fn new(state: TcpState, now_ms: u64) -> Self {
        Self {
            state,
            closed: false,
            always_keepalive: false,
            keepalive: false,
            receiver_closed: false,
            fast_finwait2_recycle: false,
            sackholes: 0,
            snd_una: 0,
            snd_nxt: 0,
            snd_max: 0,
            snd_recover: 0,
            rcv_nxt: 0,
            acktime_ms: 0,
            rcvtime_ms: now_ms,
            rtt: RtoEstimator::new(TCPTV_RTOBASE_MS, TCPTV_MIN_MS, TCPTV_REXMTMAX_MS, 1),
            rttmin_ms: TCPTV_MIN_MS,
            rexmit_slop_ms: TCPTV_CPU_VAR_MS,
            rexmit_initial_ms: TCPTV_RTOBASE_MS,
            rexmit_max_ms: TCPTV_REXMTMAX_MS,
            retries: TCP_MAXRXTSHIFT,
            rexmit_drop_options: false,
            maxunacktime_ms: 0,
            rxtshift: 0,
            rxtcur_ms: TCPTV_RTOBASE_MS,
            rtttime_active: false,
            acknow: false,
            keepinit_ms: TCPTV_KEEP_INIT_MS,
            keepidle_ms: TCPTV_KEEP_IDLE_MS,
            keepintvl_ms: TCPTV_KEEPINTVL_MS,
            keepcnt: TCPTV_KEEPCNT,
            rexmt_deadline_ms: None,
            keep_deadline_ms: None,
            two_msl_deadline_ms: None,
        }
    }

    /// `TP_MAXIDLE(tp)`: `TP_KEEPCNT(tp) * TP_KEEPINTVL(tp)`.
    #[must_use]
    pub fn maxidle_ms(&self) -> u64 {
        let count = if self.keepcnt == 0 {
            TCPTV_KEEPCNT
        } else {
            self.keepcnt
        };
        count.saturating_mul(self.keepintvl_ms)
    }

    /// Arm `TT_KEEP` for the connect path (`tcp_usrreq.c`: `TP_KEEPINIT(tp)` on SYN states).
    pub fn arm_keep_init(&mut self, now_ms: u64) {
        self.keep_deadline_ms = Some(now_ms.saturating_add(self.keepinit_ms));
    }

    /// Arm `TT_2MSL` the way `tcp_twstart()`/`tcp_input.c` do: `2 * MSL` in TIME-WAIT, the fast
    /// FIN_WAIT_2 timeout or `TP_MAXIDLE` in FIN_WAIT_2.
    pub fn arm_two_msl(&mut self, now_ms: u64, msl_ms: u64) {
        let timeout = match self.state {
            TcpState::FinWait2 if self.fast_finwait2_recycle => TCPTV_FINWAIT2_TIMEOUT_MS,
            TcpState::FinWait2 => self.maxidle_ms(),
            _ => msl_ms.saturating_mul(2),
        };
        self.two_msl_deadline_ms = Some(now_ms.saturating_add(timeout));
    }

    /// FreeBSD `tcp_maxunacktime_check()`: `true` when progress has stalled.
    #[must_use]
    pub fn maxunacktime_exceeded(&self, now_ms: u64) -> bool {
        self.maxunacktime_ms != 0
            && self.acktime_ms != 0
            && now_ms >= self.acktime_ms.saturating_add(self.maxunacktime_ms)
    }
}

/// The result of FreeBSD `tcp_timer_rexmt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RexmtOutcome {
    /// What the caller must do next.
    pub action: RexmtAction,
}

/// The action requested by FreeBSD `tcp_timer_rexmt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RexmtAction {
    /// The callout had not expired, or the connection was already closed.
    Ignored,
    /// Retransmit from `snd_una`; `TT_REXMT` is rearmed to `t_rxtcur`.
    Retransmit {
        /// The new `t_rxtcur` in milliseconds.
        rxtcur_ms: u64,
        /// The retransmit is a SYN (`TCPS_SYN_SENT`/`TCPS_SYN_RECEIVED`).
        syn: bool,
        /// The first retransmit: save `cwnd`/`ssthresh` for a spurious-retransmit undo.
        first: bool,
        /// `tcp_resend_sackholes()` must run instead of `tcp_free_sackholes()`.
        resend_sackholes: bool,
        /// The `tcp_rexmit_drop_options` SYN workaround fires on this shift.
        drop_options: bool,
        /// `t_rxtshift > TCP_RTT_INVALIDATE`: the next RTT sample starts a new estimate.
        rtt_invalidated: bool,
        /// Enter PLPMTUD black-hole detection (K270 owns the MSS ladder).
        blackhole_detect: bool,
    },
    /// More than `V_tcp_retries` retransmits, or `tcp_maxunacktime` expired: drop.
    Drop {
        /// Always `ETIMEDOUT` in FreeBSD `tcp_timer_rexmt`.
        error: Errno,
    },
}

/// The result of FreeBSD `tcp_timer_keep`.
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
    /// Keepalive is off (or the state is past CLOSING): `TT_KEEP` is rearmed to `TP_KEEPIDLE`.
    Rearm,
    /// The connection was dropped.
    Drop {
        /// Always `ETIMEDOUT` in FreeBSD `tcp_timer_keep`.
        error: Errno,
    },
}

/// The result of FreeBSD `tcp_timer_2msl`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TwoMslOutcome {
    /// The callout had not expired, or the connection was already closed.
    Ignored,
    /// Idle inside `TP_MAXIDLE`: `TT_2MSL` is rearmed to `TP_KEEPINTVL`.
    Rearm,
    /// TIME-WAIT, fast FIN_WAIT_2 recycle, or idle past `TP_MAXIDLE`: close the connection.
    Close,
}

/// FreeBSD `tcp_timer_rexmt` (`sys/netinet/tcp_timer.c`).
///
/// The caller performs the retransmit (`tcp_output`), the congestion-control response
/// (`cc_cong_signal(CC_RTO)`/`cwnd = 1` for SYN), the SACK-hole action and the PLPMTUD ladder.
#[must_use]
pub fn tcp_timer_rexmt(tp: &mut TimerState, now_ms: u64) -> RexmtOutcome {
    let ignored = RexmtOutcome {
        action: RexmtAction::Ignored,
    };
    if tp.closed || !callout_expired(tp.rexmt_deadline_ms, now_ms) {
        return ignored;
    }

    tp.rxtshift = tp.rxtshift.saturating_add(1);
    if tp.rxtshift > tp.retries || tp.maxunacktime_exceeded(now_ms) {
        tp.rxtshift = tp.retries;
        drop_state(tp);
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
        // FreeBSD calls tcp_free_sackholes() for every retransmit after the first.
        tp.sackholes = 0;
    }

    let base = if syn {
        tp.rexmit_initial_ms
    } else {
        tp.rtt.rexmtval_ms()
    };
    tp.rxtcur_ms = RtoEstimator::backoff_from_ms(
        base,
        tp.rxtshift,
        tp.rttmin_ms,
        tp.rexmit_slop_ms,
        tp.rexmit_max_ms,
    );
    tp.rexmt_deadline_ms = Some(now_ms.saturating_add(tp.rxtcur_ms));

    let rtt_invalidated = tp.rxtshift > TCP_RTT_INVALIDATE;
    if rtt_invalidated {
        tp.rtt.invalidate_srtt();
    }

    let blackhole_detect = first && matches!(tp.state, TcpState::Established | TcpState::FinWait1);

    // tcp_rexmit_drop_options: the third SYN retransmit drops RFC 1323/SACK options for old
    // terminal servers with broken VJ header compression.
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

/// FreeBSD `tcp_timer_keep` (`sys/netinet/tcp_timer.c`).
#[must_use]
pub fn tcp_timer_keep(tp: &mut TimerState, now_ms: u64) -> KeepOutcome {
    if tp.closed || !callout_expired(tp.keep_deadline_ms, now_ms) {
        return KeepOutcome::Ignored;
    }

    // The keepalive callout is not reset on every ACK, so it may fire before TP_KEEPIDLE.
    if established(tp.state) {
        let idle = now_ms.saturating_sub(tp.rcvtime_ms);
        if idle < tp.keepidle_ms {
            tp.keep_deadline_ms = Some(now_ms.saturating_add(tp.keepidle_ms - idle));
            return KeepOutcome::Rearm;
        }
    }

    if !established(tp.state) {
        drop_state(tp);
        return KeepOutcome::Drop {
            error: Errno::ETIMEDOUT,
        };
    }

    if (tp.always_keepalive || tp.keepalive) && keep_state_allows_probe(tp.state) {
        if now_ms.saturating_sub(tp.rcvtime_ms) >= tp.keepidle_ms.saturating_add(tp.maxidle_ms()) {
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

/// FreeBSD `tcp_timer_2msl` (`sys/netinet/tcp_timer.c`).
#[must_use]
pub fn tcp_timer_2msl(tp: &mut TimerState, now_ms: u64) -> TwoMslOutcome {
    if tp.closed || !callout_expired(tp.two_msl_deadline_ms, now_ms) {
        return TwoMslOutcome::Ignored;
    }

    // tcp_free_sackholes(); FreeBSD does this before the state decision.
    tp.sackholes = 0;

    let force_close = tp.state == TcpState::TimeWait
        || (tp.state == TcpState::FinWait2 && tp.fast_finwait2_recycle && tp.receiver_closed);
    let close = if force_close {
        true
    } else if now_ms.saturating_sub(tp.rcvtime_ms) <= tp.maxidle_ms() {
        tp.two_msl_deadline_ms = Some(now_ms.saturating_add(tp.keepintvl_ms));
        false
    } else {
        true
    };

    if close {
        drop_state(tp);
        TwoMslOutcome::Close
    } else {
        TwoMslOutcome::Rearm
    }
}

/// FreeBSD `TCPS_HAVEESTABLISHED`: the state is ESTABLISHED or later.
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

/// FreeBSD `tcp_timer_keep`'s `t_state <= TCPS_CLOSING`.
#[must_use]
pub const fn keep_state_allows_probe(state: TcpState) -> bool {
    matches!(
        state,
        TcpState::Established
            | TcpState::FinWait1
            | TcpState::FinWait2
            | TcpState::CloseWait
            | TcpState::Closing
    )
}

/// FreeBSD `callout_expired()`: an armed deadline at or before `now`.
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
