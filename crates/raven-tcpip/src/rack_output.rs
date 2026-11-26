// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD `sys/netinet/tcp_stacks/rack.c` at
// 133344405f532bd54ff3629a77d4c4e062f01fdb (BSD-2-Clause):
//   rack_output, tcp_rack_output, rack_timer_start/cancel/audit,
//   rack_timeout_delack/rack/tlp/rxt/persist/keepalive,
//   rack_remxt_tmr, rack_stopall, rack_stop_all_timers, rack_set_state,
//   rack_handoff_ok, rack_enter_persist, rack_exit_persist,
//   rack_start_hpts_timer, rack_process_timers, rack_get_persists_timer_val.
//
// The kernel tcpcb/mbuf/HPTS pointers and locks are mapped to the K314 RackOs
// and K313 RackHpts software contexts. This is a sans-IO behavioral port, not
// a kernel-callout copy.

//! RACK output, timer and state-handoff functions.
//!
//! This module owns the donor output/timer group. It uses the K313 HPTS wheel,
//! the K314 OS/socket-buffer context and the K318 transport constants; it does
//! not edit `stack.rs`, `tcp_state.rs` or `tcp_data.rs`.

use crate::Errno;
use crate::rack_hpts::{
    PACE_TMR_DELACK, PACE_TMR_KEEP, PACE_TMR_PERSIT, PACE_TMR_RACK, PACE_TMR_RXT, PACE_TMR_TLP,
    RackHpts,
};
use crate::rack_os::RackOs;
use crate::rack_vtable::{
    TCP_FUNC_DEFAULT_OK, TCP_FUNC_OUTPUT_CANDROP, TCPS_CLOSE_WAIT, TCPS_CLOSED, TCPS_ESTABLISHED,
    TCPS_SYN_RECEIVED, TCPS_SYN_SENT, TF_SENTFIN,
};

/// FreeBSD `TCPS_FIN_WAIT_1`.
pub const TCPS_FIN_WAIT_1: u8 = 6;
/// FreeBSD `TCPS_FIN_WAIT_2`.
pub const TCPS_FIN_WAIT_2: u8 = 7;
/// FreeBSD `TCPS_CLOSING`.
pub const TCPS_CLOSING: u8 = 8;
/// FreeBSD `TCPS_LAST_ACK`.
pub const TCPS_LAST_ACK: u8 = 9;

const TLP_MAX_PROBES: u8 = 2;
const KEEP_MAX_PROBES: u8 = 9;
const RXT_MAX_SHIFT: u8 = 12;
const KEEPIDLE_US: u32 = 7_200_000;
const KEEPINTVL_US: u32 = 75_000;
const PERSIST_MIN_US: u32 = 250_000;
const PERSIST_MAX_US: u32 = 1_000_000;

/// The action selected by one timer callback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TimerResult {
    /// Bytes sent (probe or retransmit).
    pub bytes: u32,
    /// Timers that were handled.
    pub fired: u8,
    /// The connection was closed by the timer.
    pub closed: bool,
}

/// The RACK output/timer state machine.
pub struct RackOutput {
    /// K314 OS and tcpcb context.
    pub os: RackOs,
    /// K313 HPTS wheel.
    pub hpts: RackHpts,
    /// HPTS entry identifier.
    pub id: u32,
    /// Peer send window.
    pub snd_wnd: u32,
    /// Active timer bits.
    pub timers: u8,
    /// Retransmit backoff shift.
    pub rxtshift: u8,
    /// TLP probes in this flight.
    pub tlp_probes: u8,
    /// Persist is active.
    pub persist: bool,
    /// Keepalive probes sent.
    pub keepalive_probes: u8,
    /// Keepalive idle interval, microseconds.
    pub keepidle_us: u32,
    /// Keepalive probe interval, microseconds.
    pub keepintvl_us: u32,
    /// Last receive time, microseconds.
    pub last_rx_us: i64,
    /// A retransmit is pending.
    pub must_retran: bool,
    /// The application closed the send side.
    pub close_requested: bool,
    /// FIN has been sent.
    pub fin_sent: bool,
    /// New bytes sent.
    pub bytes_transmitted: u64,
    /// Retransmitted bytes.
    pub bytes_retransmitted: u64,
    /// Last action label for tests/observability.
    pub last_action: &'static str,
}

impl Default for RackOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl RackOutput {
    /// A running RACK connection in ESTABLISHED with a 10-MSS window.
    #[must_use]
    pub fn new() -> Self {
        let mut os = RackOs::new();
        os.tcpcb.t_state = TCPS_ESTABLISHED;
        os.tcpcb.snd_cwnd = 10 * 1_460;
        os.tcpcb.snd_ssthresh = u32::MAX;
        os.tcpcb.t_maxseg = 1_460;
        os.tcpcb.t_rxtcur = 1;
        let mut hpts = RackHpts::new(0);
        hpts.tcp_hpts_init(1);
        Self {
            os,
            hpts,
            id: 1,
            snd_wnd: 65_535,
            timers: 0,
            rxtshift: 0,
            tlp_probes: 0,
            persist: false,
            keepalive_probes: 0,
            keepidle_us: KEEPIDLE_US,
            keepintvl_us: KEEPINTVL_US,
            last_rx_us: 0,
            must_retran: false,
            close_requested: false,
            fin_sent: false,
            bytes_transmitted: 0,
            bytes_retransmitted: 0,
            last_action: "init",
        }
    }

    /// Queue application bytes in the K314 send buffer.
    pub fn queue(&mut self, bytes: &[u8]) -> bool {
        let Some(mbuf) = self.os.m_getcl(0) else {
            return false;
        };
        if !self.os.mbufs.write(mbuf, 0, bytes) || !self.os.mbufs.set_len(mbuf, bytes.len()) {
            return false;
        }
        let crate::RackOs { snd, mbufs, .. } = &mut self.os;
        snd.sbappendstream_locked(mbufs, Some(mbuf)).is_ok()
    }

    /// FreeBSD `rack_output`: send new data, a retransmit or FIN within the window.
    pub fn rack_output(&mut self) -> Result<(), Errno> {
        if !matches!(
            self.os.tcpcb.t_state,
            TCPS_ESTABLISHED
                | TCPS_CLOSE_WAIT
                | TCPS_FIN_WAIT_1
                | TCPS_FIN_WAIT_2
                | TCPS_CLOSING
                | TCPS_LAST_ACK
        ) {
            return Err(Errno::ENOTCONN);
        }
        if self.must_retran {
            let len = self
                .os
                .tcpcb
                .snd_max
                .wrapping_sub(self.os.tcpcb.snd_una)
                .min(u32::from(self.os.tcpcb.t_maxseg))
                .max(1);
            self.os.tcpcb.snd_nxt = self.os.tcpcb.snd_una;
            self.bytes_retransmitted = self.bytes_retransmitted.saturating_add(u64::from(len));
            self.must_retran = false;
            self.last_action = "retransmit";
            self.arm_packet_output();
            return Ok(());
        }
        let off = self.os.tcpcb.snd_max.wrapping_sub(self.os.tcpcb.snd_una);
        let sbavail = self.os.sbavail();
        let flight = off;
        let sendwin = self.snd_wnd.min(self.os.tcpcb.snd_cwnd);
        if sbavail > off && sendwin > flight {
            let len = (sbavail - off)
                .min(sendwin - flight)
                .min(u32::from(self.os.tcpcb.t_maxseg));
            if len > 0 {
                self.os.tcpcb.snd_max = self.os.tcpcb.snd_max.wrapping_add(len);
                if self.os.tcpcb.snd_nxt < self.os.tcpcb.snd_max {
                    self.os.tcpcb.snd_nxt = self.os.tcpcb.snd_max;
                }
                self.bytes_transmitted = self.bytes_transmitted.saturating_add(u64::from(len));
                self.os.tcpcb.t_sndtime =
                    u32::try_from(self.os.tcp_get_usecs() / 1_000).unwrap_or(0);
                self.last_action = "send";
                self.arm_rxt();
                self.arm_packet_output();
                return Ok(());
            }
        }
        if self.close_requested && !self.fin_sent && self.os.sbavail() == off {
            self.os.tcpcb.snd_nxt = self.os.tcpcb.snd_nxt.wrapping_add(1);
            self.os.tcpcb.snd_max = self.os.tcpcb.snd_max.wrapping_add(1);
            self.os.tcpcb.t_flags |= TF_SENTFIN;
            self.fin_sent = true;
            self.last_action = "fin";
            self.arm_rxt();
            self.arm_packet_output();
            return Ok(());
        }
        if self.os.sbavail() > off && sendwin <= flight {
            self.rack_enter_persist();
            self.last_action = "persist";
        } else {
            self.last_action = "idle";
        }
        Ok(())
    }

    /// FreeBSD `tcp_rack_output`: the transport wrapper.
    #[must_use]
    pub fn tcp_rack_output(&mut self) -> i32 {
        if self.rack_output().is_ok() {
            i32::try_from(TCP_FUNC_DEFAULT_OK).unwrap_or(0)
        } else {
            i32::try_from(TCP_FUNC_OUTPUT_CANDROP).unwrap_or(1)
        }
    }

    fn arm_packet_output(&mut self) {
        let usecs = u64::from(self.os.pacing.divisor.max(1)) * 10;
        let _ = self
            .hpts
            .rack_timer_start(self.id, PACE_TMR_RACK, usecs.max(1));
        self.timers |= PACE_TMR_RACK;
    }

    fn arm_rxt(&mut self) {
        let usecs = self.rto_us().saturating_mul(1 << self.rxtshift.min(8));
        let _ = self.hpts.rack_timer_start(self.id, PACE_TMR_RXT, usecs);
        self.timers |= PACE_TMR_RXT;
    }

    fn rto_us(&self) -> u64 {
        u64::from(self.os.tcpcb.t_rxtcur.max(1)) * 1_000
    }

    /// FreeBSD `rack_get_persists_timer_val`.
    #[must_use]
    pub fn rack_get_persists_timer_val(&self) -> u32 {
        let base = u32::try_from(self.rto_us()).unwrap_or(u32::MAX);
        let shifted = base.saturating_mul(1u32 << u32::from(self.rxtshift.min(6)));
        shifted.clamp(PERSIST_MIN_US, PERSIST_MAX_US)
    }

    /// FreeBSD `rack_timer_start`.
    pub fn rack_timer_start(&mut self, timers: u8, usecs: u64) -> u64 {
        let slot = self.hpts.rack_timer_start(self.id, timers, usecs.max(1));
        if slot != 0 {
            self.timers |= timers;
        }
        self.last_action = "timer_start";
        slot
    }

    /// FreeBSD `rack_start_hpts_timer`.
    pub fn rack_start_hpts_timer(&mut self, usecs: u64, timers: u8) -> u64 {
        self.rack_timer_start(timers, usecs)
    }

    /// FreeBSD `rack_timer_cancel`.
    pub fn rack_timer_cancel(&mut self) -> u8 {
        let flags = self.hpts.rack_timer_cancel(self.id);
        self.timers &= !flags;
        self.last_action = "timer_cancel";
        flags
    }

    /// FreeBSD `rack_timer_audit`: return the timers due at `now_us`.
    pub fn rack_timer_audit(&mut self, now_us: u64) -> u8 {
        let callbacks = self.hpts.tcp_hpts_softclock(now_us);
        let mut fired = 0u8;
        for callback in callbacks {
            fired |= callback.timers;
        }
        self.timers &= !fired;
        fired
    }

    /// FreeBSD `rack_remxt_tmr`.
    pub fn rack_remxt_tmr(&mut self) -> u8 {
        let flags = self.hpts.rack_remxt_tmr(self.id);
        if flags & PACE_TMR_RXT != 0 {
            self.rxtshift = self.rxtshift.saturating_add(1).min(RXT_MAX_SHIFT);
            self.must_retran = true;
            self.arm_rxt();
        }
        self.last_action = "remxt";
        flags
    }

    /// FreeBSD `rack_stopall`.
    pub fn rack_stopall(&mut self) -> u8 {
        let flags = self.hpts.rack_stopall(self.id);
        self.timers = 0;
        self.persist = false;
        self.last_action = "stopall";
        flags
    }

    /// FreeBSD `rack_stop_all_timers`.
    pub fn rack_stop_all_timers(&mut self) -> u8 {
        let flags = self.hpts.rack_stop_all_timers(self.id);
        self.timers &= !flags;
        self.last_action = "stop_all";
        flags
    }

    /// FreeBSD `rack_enter_persist`.
    pub fn rack_enter_persist(&mut self) -> bool {
        if self.persist || self.timers & PACE_TMR_RXT != 0 {
            return false;
        }
        self.persist = true;
        let usecs = u64::from(self.rack_get_persists_timer_val());
        let _ = self.hpts.rack_timer_start(self.id, PACE_TMR_PERSIT, usecs);
        self.timers |= PACE_TMR_PERSIT;
        self.last_action = "enter_persist";
        true
    }

    /// FreeBSD `rack_exit_persist`.
    pub fn rack_exit_persist(&mut self) -> bool {
        if !self.persist {
            return false;
        }
        self.persist = false;
        let _ = self.hpts.rack_timer_cancel(self.id);
        self.timers &= !PACE_TMR_PERSIT;
        self.last_action = "exit_persist";
        true
    }

    /// FreeBSD `rack_timeout_delack`.
    pub fn rack_timeout_delack(&mut self) -> bool {
        if self.timers & PACE_TMR_DELACK == 0 {
            return false;
        }
        self.timers &= !PACE_TMR_DELACK;
        self.os.tcpcb.t_flags |= 0x0020; // TF_ACKNOW from tcp_var.h.
        self.last_action = "delack";
        true
    }

    /// FreeBSD `rack_timeout_rack`.
    pub fn rack_timeout_rack(&mut self) -> bool {
        if self.timers & PACE_TMR_RACK == 0 {
            return false;
        }
        self.timers &= !PACE_TMR_RACK;
        if self.os.tcpcb.snd_una == self.os.tcpcb.snd_max {
            return false;
        }
        self.must_retran = true;
        self.last_action = "timeout_rack";
        true
    }

    /// FreeBSD `rack_timeout_tlp`.
    pub fn rack_timeout_tlp(&mut self) -> TimerResult {
        let mut result = TimerResult::default();
        if self.timers & PACE_TMR_TLP == 0 {
            return result;
        }
        self.timers &= !PACE_TMR_TLP;
        result.fired = PACE_TMR_TLP;
        if self.os.tcpcb.snd_una == self.os.tcpcb.snd_max || self.tlp_probes >= TLP_MAX_PROBES {
            return result;
        }
        self.tlp_probes = self.tlp_probes.saturating_add(1);
        self.os.tcpcb.snd_max = self.os.tcpcb.snd_max.wrapping_add(1);
        result.bytes = 1;
        let usecs = self.rto_us().saturating_mul(2);
        let _ = self
            .hpts
            .rack_timer_start(self.id, PACE_TMR_TLP, usecs.max(1));
        self.timers |= PACE_TMR_TLP;
        self.last_action = "timeout_tlp";
        result
    }

    /// FreeBSD `rack_timeout_rxt`.
    pub fn rack_timeout_rxt(&mut self) -> TimerResult {
        let mut result = TimerResult::default();
        if self.timers & PACE_TMR_RXT == 0 {
            return result;
        }
        self.timers &= !PACE_TMR_RXT;
        result.fired = PACE_TMR_RXT;
        if self.rxtshift >= RXT_MAX_SHIFT {
            self.os.tcpcb.t_state = TCPS_CLOSED;
            self.rack_stopall();
            result.closed = true;
            self.last_action = "timeout_rxt_closed";
            return result;
        }
        self.rxtshift = self.rxtshift.saturating_add(1);
        self.must_retran = true;
        self.arm_rxt();
        self.last_action = "timeout_rxt";
        result.bytes = self
            .os
            .tcpcb
            .snd_max
            .wrapping_sub(self.os.tcpcb.snd_una)
            .min(u32::from(self.os.tcpcb.t_maxseg));
        result
    }

    /// FreeBSD `rack_timeout_persist`.
    pub fn rack_timeout_persist(&mut self) -> TimerResult {
        let mut result = TimerResult::default();
        if self.timers & PACE_TMR_PERSIT == 0 {
            return result;
        }
        self.timers &= !PACE_TMR_PERSIT;
        result.fired = PACE_TMR_PERSIT;
        if self.snd_wnd > 0 {
            self.rack_exit_persist();
            return result;
        }
        if self.os.sbavail() > self.os.tcpcb.snd_max.wrapping_sub(self.os.tcpcb.snd_una) {
            self.os.tcpcb.snd_max = self.os.tcpcb.snd_max.wrapping_add(1);
            result.bytes = 1;
        }
        self.rxtshift = self.rxtshift.saturating_add(1).min(RXT_MAX_SHIFT);
        let usecs = u64::from(self.rack_get_persists_timer_val());
        let _ = self.hpts.rack_timer_start(self.id, PACE_TMR_PERSIT, usecs);
        self.timers |= PACE_TMR_PERSIT;
        self.last_action = "timeout_persist";
        result
    }

    /// FreeBSD `rack_timeout_keepalive`.
    pub fn rack_timeout_keepalive(&mut self, now_us: i64) -> TimerResult {
        let mut result = TimerResult::default();
        if self.timers & PACE_TMR_KEEP == 0 {
            return result;
        }
        self.timers &= !PACE_TMR_KEEP;
        result.fired = PACE_TMR_KEEP;
        let idle = now_us.saturating_sub(self.last_rx_us);
        if idle < i64::from(self.keepidle_us) {
            let _ = self.hpts.rack_timer_start(
                self.id,
                PACE_TMR_KEEP,
                u64::from(self.keepidle_us - u32::try_from(idle).unwrap_or(0)),
            );
            self.timers |= PACE_TMR_KEEP;
            return result;
        }
        self.keepalive_probes = self.keepalive_probes.saturating_add(1);
        result.bytes = 1;
        if self.keepalive_probes > KEEP_MAX_PROBES {
            self.os.tcpcb.t_state = TCPS_CLOSED;
            self.rack_stopall();
            result.closed = true;
            self.last_action = "keepalive_closed";
            return result;
        }
        let _ = self
            .hpts
            .rack_timer_start(self.id, PACE_TMR_KEEP, u64::from(self.keepintvl_us));
        self.timers |= PACE_TMR_KEEP;
        self.last_action = "keepalive";
        result
    }

    /// FreeBSD `rack_process_timers`: dispatch the donor order.
    pub fn rack_process_timers(&mut self, flags: u8, now_us: i64) -> TimerResult {
        let mut result = TimerResult::default();
        if flags & PACE_TMR_DELACK != 0 && self.rack_timeout_delack() {
            result.fired |= PACE_TMR_DELACK;
        }
        if flags & PACE_TMR_RACK != 0 && self.rack_timeout_rack() {
            result.fired |= PACE_TMR_RACK;
        }
        if flags & PACE_TMR_TLP != 0 {
            let tlp = self.rack_timeout_tlp();
            result.fired |= tlp.fired;
            result.bytes = result.bytes.saturating_add(tlp.bytes);
        }
        if flags & PACE_TMR_RXT != 0 {
            let rxt = self.rack_timeout_rxt();
            result.fired |= rxt.fired;
            result.bytes = result.bytes.saturating_add(rxt.bytes);
            result.closed |= rxt.closed;
        }
        if flags & PACE_TMR_PERSIT != 0 {
            let persist = self.rack_timeout_persist();
            result.fired |= persist.fired;
            result.bytes = result.bytes.saturating_add(persist.bytes);
        }
        if flags & PACE_TMR_KEEP != 0 {
            let keep = self.rack_timeout_keepalive(now_us);
            result.fired |= keep.fired;
            result.bytes = result.bytes.saturating_add(keep.bytes);
            result.closed |= keep.closed;
        }
        self.last_action = "process_timers";
        result
    }

    /// FreeBSD `rack_set_state`.
    pub fn rack_set_state(&mut self, state: u8) -> bool {
        let allowed = matches!(
            state,
            TCPS_CLOSED
                | TCPS_SYN_SENT
                | TCPS_SYN_RECEIVED
                | TCPS_ESTABLISHED
                | TCPS_CLOSE_WAIT
                | TCPS_FIN_WAIT_1
                | TCPS_FIN_WAIT_2
                | TCPS_CLOSING
                | TCPS_LAST_ACK
        );
        if !allowed {
            return false;
        }
        self.os.tcpcb.t_state = state;
        if state == TCPS_CLOSED {
            self.rack_stopall();
        } else if state == TCPS_ESTABLISHED {
            self.keepalive_probes = 0;
            let _ = self
                .hpts
                .rack_timer_start(self.id, PACE_TMR_KEEP, u64::from(self.keepidle_us));
            self.timers |= PACE_TMR_KEEP;
        }
        self.last_action = "set_state";
        true
    }

    /// FreeBSD `rack_handoff_ok`: the donor only hands off a synchronized,
    /// non-FIN transport.
    pub fn rack_handoff_ok(&self) -> Result<(), Errno> {
        if matches!(
            self.os.tcpcb.t_state,
            TCPS_ESTABLISHED | TCPS_CLOSE_WAIT | TCPS_FIN_WAIT_1 | TCPS_FIN_WAIT_2
        ) && !self.fin_sent
        {
            Ok(())
        } else {
            Err(Errno::EINVAL)
        }
    }

    /// A queue helper for tests and future stack wiring.
    pub fn queued(&self) -> u32 {
        self.os.sbavail()
    }

    /// A synthetic received-byte timestamp update.
    pub fn note_receive(&mut self, now_us: i64) {
        self.last_rx_us = now_us;
        self.keepalive_probes = 0;
    }

    /// The last packet output timer arm.
    pub fn packet_output_armed(&self) -> bool {
        self.timers & PACE_TMR_RACK != 0
    }
}

#[cfg(test)]
#[path = "rack_output_ut.rs"]
mod tests;
