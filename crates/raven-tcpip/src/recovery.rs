// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! RACK-TLP loss detection, spurious-retransmission undo and ECN state.

use alloc::vec::Vec;

use crate::Duration;

/// One transmitted range tracked by RACK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RackEntry {
    /// Inclusive start sequence.
    pub start: u32,
    /// Exclusive end sequence.
    pub end: u32,
    /// Transmission timestamp in milliseconds.
    pub xmit_ts: u32,
    /// The range was SACKed.
    pub sacked: bool,
    /// The range has been retransmitted.
    pub retransmitted: bool,
}

/// Inputs for one RACK ACK/recovery event.
#[derive(Clone, Copy, Debug, Default)]
pub struct RackContext {
    /// Current monotonic time in milliseconds.
    pub now_ms: u32,
    /// Smoothed RTT in milliseconds.
    pub srtt_ms: u32,
    /// Minimum RTT in milliseconds.
    pub min_rtt_ms: u32,
    /// Current sender MSS.
    pub mss: u32,
    /// Cumulative ACK.
    pub ack: u32,
    /// Highest sequence sent.
    pub snd_nxt: u32,
    /// Congestion window before this event.
    pub cwnd: u32,
    /// Slow-start threshold.
    pub ssthresh: u32,
}

/// Result of one RACK ACK/recovery event.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RackOutcome {
    /// Bytes newly acknowledged.
    pub acked: u32,
    /// Ranges declared lost, in sequence order.
    pub lost: Vec<u32>,
    /// The recovery episode ended.
    pub recovered: bool,
    /// The sender should retransmit `lost`.
    pub retransmit: bool,
}

/// Per-segment RACK transmission bookkeeping.
#[derive(Clone, Debug, Default)]
pub struct Rack {
    /// Latest transmission timestamp seen, in milliseconds.
    pub xmit_ts: u32,
    /// Highest sequence covered by the latest transmission.
    pub end_seq: u32,
    /// Reordering window in milliseconds.
    pub reorder_window: u32,
    /// Whether a DSACK has grown the window.
    pub dsack_grown: bool,
    /// Outstanding transmitted ranges.
    pub entries: Vec<RackEntry>,
    /// Cumulative ACK at the last event.
    pub snd_una: u32,
    /// Recovery point at entry.
    pub recover: u32,
    /// True while a RACK recovery episode is active.
    pub in_recovery: bool,
    /// Smallest observed RTT.
    pub min_rtt_ms: u32,
}

impl Rack {
    /// Create a RACK detector with a reordering window.
    #[must_use]
    pub const fn new(reorder_window_ms: u32) -> Self {
        Self {
            xmit_ts: 0,
            end_seq: 0,
            reorder_window: reorder_window_ms,
            dsack_grown: false,
            entries: Vec::new(),
            snd_una: 0,
            recover: 0,
            in_recovery: false,
            min_rtt_ms: 0,
        }
    }

    /// Record a transmission.
    pub fn on_sent(&mut self, end_seq: u32, now_ms: u32) {
        if now_ms >= self.xmit_ts {
            self.xmit_ts = now_ms;
            self.end_seq = end_seq;
        }
    }

    /// Declare loss by time: the segment is older than the reordering window.
    #[must_use]
    pub fn is_lost(&self, now_ms: u32, srtt_ms: u32) -> bool {
        let bound = (srtt_ms / 4).max(self.reorder_window).max(1);
        now_ms.saturating_sub(self.xmit_ts) > bound
    }

    /// Grow the reordering window after a DSACK, bounded by the RTT.
    pub fn on_dsack(&mut self, srtt_ms: u32) {
        self.reorder_window = self
            .reorder_window
            .saturating_add((srtt_ms / 4).max(1))
            .min(srtt_ms.max(1));
        self.dsack_grown = true;
    }

    /// Reset after an ACK advances the recovery point.
    pub fn on_ack(&mut self, end_seq: u32, now_ms: u32) {
        self.end_seq = end_seq;
        self.xmit_ts = now_ms;
    }

    /// Insert or update one transmitted range.
    pub fn send(&mut self, start: u32, end: u32, now_ms: u32) {
        if end <= start {
            return;
        }
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.start == start && entry.end == end)
        {
            entry.xmit_ts = now_ms;
            entry.retransmitted = true;
        } else {
            self.entries.push(RackEntry {
                start,
                end,
                xmit_ts: now_ms,
                sacked: false,
                retransmitted: false,
            });
            self.entries.sort_by_key(|entry| entry.start);
        }
        self.on_sent(end, now_ms);
    }

    /// Mark a range SACKed.
    pub fn mark_sacked(&mut self, start: u32, end: u32) {
        for entry in &mut self.entries {
            if entry.start == start && entry.end == end {
                entry.sacked = true;
            }
        }
    }

    /// FreeBSD `rack_ack_received` core: consume a cumulative ACK, age the reordering window,
    /// declare losses and enter or leave recovery.
    pub fn ack_received(&mut self, ctx: &RackContext) -> RackOutcome {
        let mut outcome = RackOutcome::default();
        if ctx.ack.wrapping_sub(self.snd_una) < 0x8000_0000 {
            outcome.acked = ctx.ack.wrapping_sub(self.snd_una);
        }
        if ctx.ack <= self.snd_una {
            return outcome;
        }
        self.snd_una = ctx.ack;
        self.entries.retain(|entry| entry.end > ctx.ack);
        if self.min_rtt_ms == 0 || ctx.srtt_ms < self.min_rtt_ms {
            self.min_rtt_ms = ctx.srtt_ms;
        }

        if self.in_recovery && ctx.ack >= self.recover {
            self.in_recovery = false;
            outcome.recovered = true;
            return outcome;
        }

        let bound = (ctx.srtt_ms / 4).max(self.reorder_window).max(1);
        for entry in &self.entries {
            if !entry.sacked
                && !entry.retransmitted
                && ctx.now_ms.saturating_sub(entry.xmit_ts) > bound
            {
                outcome.lost.push(entry.start);
            }
        }
        if !outcome.lost.is_empty() && !self.in_recovery {
            self.in_recovery = true;
            self.recover = ctx.snd_nxt;
            outcome.retransmit = true;
        }
        outcome
    }

    /// FreeBSD `rack_update_entry` core: retransmit part of a range at `now_ms`.
    pub fn update_entry(&mut self, start: u32, len: u32, now_ms: u32) {
        if len == 0 {
            return;
        }
        for entry in &mut self.entries {
            if entry.start == start {
                entry.xmit_ts = now_ms;
                entry.retransmitted = true;
                return;
            }
        }
        self.send(start, start.saturating_add(len), now_ms);
    }

    /// FreeBSD `rack_post_recovery` core: return the recovery-exit window.
    pub fn post_recovery(&self, ctx: &RackContext, pipe: u32) -> u32 {
        if pipe < ctx.ssthresh {
            pipe.max(ctx.mss).saturating_add(ctx.mss)
        } else {
            ctx.ssthresh
        }
    }

    /// Update the reordering window from an observed ACK gap, bounded by SRTT.
    pub fn observe_reordering(&mut self, srtt_ms: u32) {
        if srtt_ms == 0 {
            return;
        }
        self.reorder_window = self
            .reorder_window
            .saturating_add((srtt_ms / 4).max(1))
            .min(srtt_ms);
    }
}

/// Tail-loss-probe timing.
#[derive(Clone, Debug, Default)]
pub struct Tlp {
    /// Current probe deadline in milliseconds, if armed.
    pub deadline: Option<u32>,
}

impl Tlp {
    /// Arm the PTO for `now` when there is unacknowledged data.
    pub fn arm(&mut self, now_ms: u32, srtt_ms: u32, rttvar_ms: u32) {
        let pto = srtt_ms
            .saturating_add(4u32.saturating_mul(rttvar_ms).max(1))
            .saturating_add(2);
        self.deadline = Some(now_ms.saturating_add(pto));
    }

    /// Disarm on ACK.
    pub fn disarm(&mut self) {
        self.deadline = None;
    }

    /// TLP fires at its deadline.
    #[must_use]
    pub fn fires(&self, now_ms: u32) -> bool {
        self.deadline.is_some_and(|deadline| deadline <= now_ms)
    }

    /// Whether the TLP is sooner than an RTO deadline.
    #[must_use]
    pub fn is_sooner_than(&self, rto_deadline: Option<u32>) -> bool {
        match (self.deadline, rto_deadline) {
            (Some(tlp), Some(rto)) => tlp < rto,
            (Some(_), None) => true,
            _ => false,
        }
    }
}

/// Explicit-congestion-notification state.
#[derive(Clone, Debug, Default)]
pub struct Ecn {
    /// Both endpoints offered ECN.
    pub negotiated: bool,
    /// A CE mark has been observed in this RTT.
    pub ce_seen: bool,
    /// The congestion response has already run in this RTT.
    pub reacted: bool,
    /// ECE should be set on outgoing ACKs.
    pub ece_pending: bool,
    /// A CWR should be sent.
    pub cwr_pending: bool,
}

impl Ecn {
    /// Negotiate from the two SYN offer flags.
    pub fn negotiate(&mut self, local_offered: bool, peer_offered: bool) {
        self.negotiated = local_offered && peer_offered;
    }

    /// Observe a CE-marked packet; returns whether a reaction is due now.
    pub fn on_ce(&mut self) -> bool {
        if !self.negotiated {
            return false;
        }
        self.ce_seen = true;
        self.ece_pending = true;
        if !self.reacted {
            self.reacted = true;
            self.cwr_pending = true;
            true
        } else {
            false
        }
    }

    /// The beginning of a new RTT clears the once-per-RTT latch.
    pub fn on_rtt(&mut self) {
        self.reacted = false;
        self.ce_seen = false;
    }

    /// Consume the pending CWR.
    pub fn take_cwr(&mut self) -> bool {
        let value = self.cwr_pending;
        self.cwr_pending = false;
        value
    }
}

/// Saved congestion state for undoing a spurious retransmission.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpuriousUndo {
    /// Congestion window before the loss response.
    pub cwnd: u32,
    /// Slow-start threshold before the loss response.
    pub ssthresh: u32,
    /// Whether an undo is available.
    pub armed: bool,
}

impl SpuriousUndo {
    /// Record the pre-loss state.
    #[must_use]
    pub const fn record(cwnd: u32, ssthresh: u32) -> Self {
        Self {
            cwnd,
            ssthresh,
            armed: true,
        }
    }

    /// Restore the pre-loss state, returning `(cwnd, ssthresh)` once.
    pub fn restore(&mut self) -> Option<(u32, u32)> {
        if !self.armed {
            return None;
        }
        self.armed = false;
        Some((self.cwnd, self.ssthresh))
    }
}

/// RTT-derived TLP PTO in milliseconds.
#[must_use]
pub fn pto(srtt_ms: u32, rttvar_ms: u32) -> Duration {
    Duration::from_millis(u64::from(
        srtt_ms
            .saturating_add(4u32.saturating_mul(rttvar_ms).max(1))
            .saturating_add(2),
    ))
}
