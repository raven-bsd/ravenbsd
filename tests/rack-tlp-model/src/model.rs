// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Independent RACK-TLP, DSACK-reordering and spurious-undo reference model.
//!
//! Behaviour follows RFC 8985, RFC 6675 and RFC 3522 and the FreeBSD `rack.c` paths named in
//! the ticket. The model is original Rust; no donor expression is copied. It is the oracle for
//! the future full `Rack` implementation, not a second copy of the current four-field detector.

/// Model timing and probe configuration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// Smoothed RTT, milliseconds.
    pub srtt_ms: u32,
    /// RTT variation, milliseconds.
    pub rttvar_ms: u32,
    /// Retransmission timeout ceiling, milliseconds.
    pub rto_ms: u32,
    /// Maximum TLP probes per flight (RFC 8985 uses two).
    pub max_probes: u8,
    /// Delayed-ACK budget used by the two-segment TLP threshold, milliseconds.
    pub delayed_ack_ms: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            srtt_ms: 100,
            rttvar_ms: 10,
            rto_ms: 1_000,
            max_probes: 2,
            delayed_ack_ms: 40,
        }
    }
}

/// One transmitted segment in the send map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    /// Start sequence.
    pub start: u32,
    /// End sequence (exclusive).
    pub end: u32,
    /// Transmission time, milliseconds.
    pub xmit_ms: u32,
    /// Retransmission count.
    pub retransmits: u8,
}

/// The model's expected action for one event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// Nothing to do.
    Noop,
    /// Arm the TLP at the supplied deadline.
    Arm(u32),
    /// Fire a TLP probe for `seq`; `probe` is 1 or 2.
    Fire {
        /// Highest unacknowledged sequence.
        seq: u32,
        /// Probe number in this flight.
        probe: u8,
    },
    /// Disarm the TLP.
    Disarm,
    /// Grow the reordering window.
    GrowReoWnd {
        /// Previous value, milliseconds.
        from: u32,
        /// New value, milliseconds.
        to: u32,
    },
    /// Restore the congestion state after a spurious retransmission.
    Undo {
        /// Saved congestion window.
        cwnd: u32,
        /// Saved slow-start threshold.
        ssthresh: u32,
    },
    /// Remove reneged SACK ranges.
    ClearSacked {
        /// Number of removed ranges.
        ranges: usize,
    },
}

/// The result of one DSACK.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DsackOutcome {
    /// The DSACK covered the last TLP probe.
    pub was_tlp: bool,
    /// Reordering-window growth, if any.
    pub grow: Option<Action>,
    /// Spurious-undo restore, if the retransmitted bytes are now covered.
    pub undo: Option<Action>,
}

/// The independent RACK-TLP reference model.
#[derive(Clone, Debug)]
pub struct RackModel {
    /// Timing configuration.
    pub config: Config,
    /// Last event time, milliseconds.
    pub now_ms: u32,
    /// Oldest unacknowledged sequence.
    pub snd_una: u32,
    /// Highest sequence sent.
    pub snd_max: u32,
    /// Send map.
    pub segments: Vec<Segment>,
    /// SACKed ranges, sorted and merged.
    pub sacked: Vec<(u32, u32)>,
    /// Reordering window, milliseconds.
    pub reo_wnd_ms: u32,
    /// TLP deadline, milliseconds.
    pub tlp_deadline: Option<u32>,
    /// TLP probes in this flight.
    pub probes: u8,
    /// Total TLP fires.
    pub tlp_fires: usize,
    /// Last TLP probe range.
    pub last_tlp: Option<(u32, u32)>,
    /// DSACKs in the current round.
    pub dsack_round_seen: bool,
    /// DSACK bytes since the last recovery.
    pub dsack_bytes: u32,
    /// Bytes retransmitted during the current recovery.
    pub retrans_during_recovery: u32,
    /// Recovery state, if in recovery.
    pub recovery: Option<u32>,
    /// Saved congestion state for a spurious undo.
    pub undo: Option<(u32, u32)>,
    /// Congestion window.
    pub cwnd: u32,
    /// Slow-start threshold.
    pub ssthresh: u32,
    /// Successful undos.
    pub undo_count: usize,
    /// Reneged bytes.
    pub reneged_bytes: u32,
}

impl RackModel {
    /// Create a model with the supplied timing and the initial congestion state.
    #[must_use]
    pub fn new(config: Config, cwnd: u32, ssthresh: u32) -> Self {
        Self {
            config,
            now_ms: 0,
            snd_una: 0,
            snd_max: 0,
            segments: Vec::new(),
            sacked: Vec::new(),
            reo_wnd_ms: 0,
            tlp_deadline: None,
            probes: 0,
            tlp_fires: 0,
            last_tlp: None,
            dsack_round_seen: false,
            dsack_bytes: 0,
            retrans_during_recovery: 0,
            recovery: None,
            undo: None,
            cwnd,
            ssthresh,
            undo_count: 0,
            reneged_bytes: 0,
        }
    }

    /// RFC 8985 PTO: `SRTT + max(4*RTTVAR, 1) + 2`, capped by RTO.
    #[must_use]
    pub fn pto(&self) -> u32 {
        self.config
            .srtt_ms
            .saturating_add(4u32.saturating_mul(self.config.rttvar_ms).max(1))
            .saturating_add(2)
            .min(self.config.rto_ms.max(1))
            .max(1)
    }

    /// FreeBSD `rack_calc_thresh_tlp` two-segment threshold, capped by RTO.
    #[must_use]
    pub fn tlp_threshold(&self) -> u32 {
        self.config
            .srtt_ms
            .saturating_add(self.config.srtt_ms / 2)
            .saturating_add(self.config.delayed_ack_ms)
            .min(self.config.rto_ms.max(1))
            .max(1)
    }

    /// Record a transmission and arm the TLP if the flight was idle.
    pub fn on_sent(&mut self, start: u32, end: u32, now_ms: u32) -> Action {
        self.now_ms = now_ms;
        self.segments.push(Segment {
            start,
            end,
            xmit_ms: now_ms,
            retransmits: 0,
        });
        self.snd_max = self.snd_max.max(end);
        if self
            .segments
            .iter()
            .all(|segment| segment.sacked_tail(self))
        {
            return Action::Noop;
        }
        self.arm_tlp(now_ms)
    }

    /// Arm the TLP at `now + PTO` when data is outstanding.
    pub fn arm_tlp(&mut self, now_ms: u32) -> Action {
        self.now_ms = now_ms;
        if self.outstanding().is_none() {
            self.tlp_deadline = None;
            return Action::Disarm;
        }
        let deadline = now_ms.saturating_add(self.pto());
        self.tlp_deadline = Some(deadline);
        Action::Arm(deadline)
    }

    /// Poll the TLP timer.
    pub fn poll(&mut self, now_ms: u32) -> Action {
        self.now_ms = now_ms;
        let Some(deadline) = self.tlp_deadline else {
            return Action::Noop;
        };
        if now_ms < deadline {
            return Action::Noop;
        }
        let Some((start, end)) = self.outstanding() else {
            self.tlp_deadline = None;
            return Action::Disarm;
        };
        if self.probes >= self.config.max_probes {
            self.tlp_deadline = None;
            return Action::Noop;
        }
        self.probes = self.probes.saturating_add(1);
        self.tlp_fires = self.tlp_fires.saturating_add(1);
        self.last_tlp = Some((start, end));
        self.tlp_deadline = Some(now_ms.saturating_add(self.pto()));
        Action::Fire {
            seq: start,
            probe: self.probes,
        }
    }

    /// Process a cumulative ACK.
    pub fn on_ack(&mut self, ack: u32, now_ms: u32) -> Action {
        self.now_ms = now_ms;
        if ack > self.snd_una {
            self.snd_una = ack;
        }
        self.segments.retain(|segment| segment.end > self.snd_una);
        self.sacked.retain(|(_, end)| *end > self.snd_una);
        if let Some(recover) = self.recovery
            && ack >= recover
        {
            // A normal full ACK exits recovery; a later DSACK may still undo it.
            self.recovery = None;
        }
        if self.outstanding().is_none() {
            self.tlp_deadline = None;
            self.probes = 0;
            return Action::Disarm;
        }
        if self.tlp_deadline.is_none() {
            return self.arm_tlp(now_ms);
        }
        Action::Noop
    }

    /// Process a DSACK range.
    pub fn on_dsack(&mut self, start: u32, end: u32) -> DsackOutcome {
        let was_tlp = self
            .last_tlp
            .is_some_and(|(tlp_start, tlp_end)| start >= tlp_start && end <= tlp_end);
        if end > start {
            self.dsack_bytes = self.dsack_bytes.saturating_add(end - start);
        }
        let mut grow = None;
        if !was_tlp && !self.dsack_round_seen && end > start {
            self.dsack_round_seen = true;
            let from = self.reo_wnd_ms;
            let step = (self.config.srtt_ms / 4).max(1);
            self.reo_wnd_ms = self
                .reo_wnd_ms
                .saturating_add(step)
                .min(self.config.srtt_ms.max(1));
            grow = Some(Action::GrowReoWnd {
                from,
                to: self.reo_wnd_ms,
            });
        }
        let undo = if self.recovery.is_none()
            && self.retrans_during_recovery > 0
            && self.dsack_bytes >= self.retrans_during_recovery
        {
            self.restore_undo()
        } else {
            None
        };
        DsackOutcome {
            was_tlp,
            grow,
            undo,
        }
    }

    /// Start a new DSACK round.
    pub fn new_dsack_round(&mut self) {
        self.dsack_round_seen = false;
    }

    /// Enter loss recovery and snapshot the congestion state.
    pub fn enter_recovery(&mut self, recover_seq: u32) {
        self.undo = Some((self.cwnd, self.ssthresh));
        self.recovery = Some(recover_seq);
        self.retrans_during_recovery = 0;
        self.dsack_bytes = 0;
    }

    /// Note retransmitted bytes while in recovery.
    pub fn note_retransmit(&mut self, bytes: u32) {
        if self.recovery.is_some() {
            self.retrans_during_recovery = self.retrans_during_recovery.saturating_add(bytes);
        }
    }

    /// Restore the saved congestion state once.
    pub fn restore_undo(&mut self) -> Option<Action> {
        let (cwnd, ssthresh) = self.undo.take()?;
        self.cwnd = cwnd;
        self.ssthresh = ssthresh;
        self.undo_count = self.undo_count.saturating_add(1);
        self.recovery = None;
        self.retrans_during_recovery = 0;
        self.dsack_bytes = 0;
        Some(Action::Undo { cwnd, ssthresh })
    }

    /// Eifel-style spurious signal: the echoed timestamp predates the recovery.
    pub fn on_eifel(&mut self, echoed_ts: u32, recovery_ts: u32) -> Option<Action> {
        if echoed_ts < recovery_ts {
            return self.restore_undo();
        }
        None
    }

    /// Mark a SACK range.
    pub fn on_sack(&mut self, start: u32, end: u32) {
        if end <= start {
            return;
        }
        self.sacked.push((start, end));
        self.sacked.sort_unstable();
        let mut merged: Vec<(u32, u32)> = Vec::new();
        for (start, end) in self.sacked.drain(..) {
            if let Some(last) = merged.last_mut()
                && start <= last.1
            {
                last.1 = last.1.max(end);
                continue;
            }
            merged.push((start, end));
        }
        self.sacked = merged;
    }

    /// Remove reneged SACK ranges; returns the number of removed ranges.
    pub fn on_renege(&mut self, start: u32, end: u32) -> Action {
        let before = self.sacked.len();
        self.sacked.retain(|(sack_start, sack_end)| {
            let overlap = *sack_start < end && start < *sack_end;
            if overlap {
                let low = (*sack_start).max(start);
                let high = (*sack_end).min(end);
                self.reneged_bytes = self.reneged_bytes.saturating_add(high.saturating_sub(low));
            }
            !overlap
        });
        Action::ClearSacked {
            ranges: before.saturating_sub(self.sacked.len()),
        }
    }

    /// Whether `seq` is SACKed.
    #[must_use]
    pub fn is_sacked(&self, seq: u32) -> bool {
        self.sacked
            .iter()
            .any(|(start, end)| *start <= seq && seq < *end)
    }

    /// Loss by time: the segment is older than the reordering bound.
    #[must_use]
    pub fn is_lost(&self, seq: u32, now_ms: u32) -> bool {
        let bound = (self.config.srtt_ms / 4).max(self.reo_wnd_ms).max(1);
        self.segments.iter().any(|segment| {
            segment.start <= seq
                && seq < segment.end
                && !self.is_sacked(seq)
                && now_ms.saturating_sub(segment.xmit_ms) > bound
        })
    }

    /// Highest unacknowledged, non-SACKed range.
    #[must_use]
    pub fn outstanding(&self) -> Option<(u32, u32)> {
        self.segments
            .iter()
            .filter(|segment| !segment.sacked_tail(self))
            .max_by_key(|segment| segment.end)
            .map(|segment| (segment.start, segment.end))
    }

    /// SACKed byte count.
    #[must_use]
    pub fn sacked_bytes(&self) -> u32 {
        self.sacked
            .iter()
            .map(|(start, end)| end.saturating_sub(*start))
            .sum()
    }
}

impl Segment {
    fn sacked_tail(&self, model: &RackModel) -> bool {
        let mut cursor = self.start;
        for (start, end) in &model.sacked {
            if *start <= cursor && cursor < *end {
                cursor = *end;
            }
            if cursor >= self.end {
                return true;
            }
        }
        false
    }
}
