// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Reference ECN state machines.
//!
//! `RefEcn` models `recovery::Ecn` (the once-per-RTT CE latch and one-shot CWR).
//! `RefClassic` models the RFC 3168 classic path of `ecn::EcnState` in mode 2: ECE in the
//! handshake, CE -> ECE until CWR, CWR consumed on the first new-data segment, and ECT(0) on new
//! data. Accurate ECN (modes 3/4) is outside this model and reported as an open gap.

use raven_tcpip::{ECN_CE, ECN_ECT0, ECN_ECT1, ECN_MASK, ECN_NOTECT, TH_CWR, TH_ECE};

/// Reference `recovery::Ecn`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefEcn {
    /// Both endpoints offered ECN.
    pub negotiated: bool,
    /// A CE mark was seen this RTT.
    pub ce_seen: bool,
    /// The congestion response ran this RTT.
    pub reacted: bool,
    /// ECE should be set on ACKs.
    pub ece_pending: bool,
    /// A CWR should be sent.
    pub cwr_pending: bool,
}

impl RefEcn {
    /// Negotiate from the two SYN offers.
    pub fn negotiate(&mut self, local: bool, peer: bool) {
        self.negotiated = local && peer;
    }

    /// Observe CE; true only for the first CE in the RTT.
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

    /// Start a new RTT.
    pub fn on_rtt(&mut self) {
        self.reacted = false;
        self.ce_seen = false;
    }

    /// Consume the pending CWR once.
    pub fn take_cwr(&mut self) -> bool {
        let value = self.cwr_pending;
        self.cwr_pending = false;
        value
    }
}

/// Reference classic (RFC 3168) `EcnState` in mode 2.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefClassic {
    /// ECN was negotiated.
    pub permit: bool,
    /// ECE is pending on output.
    pub snd_ece: bool,
    /// CWR is pending on output.
    pub snd_cwr: bool,
    /// Use ECT(1) instead of ECT(0).
    pub use_ect1: bool,
}

/// Reference ECN counters.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RefCounters {
    /// Completed ECN handshakes.
    pub handshakes: u64,
    /// CE-marked segments received.
    pub received_ce: u64,
    /// ECT(0) segments received.
    pub received_ect0: u64,
    /// ECT(1) segments received.
    pub received_ect1: u64,
    /// ECT(0) new-data segments sent.
    pub sent_ect0: u64,
    /// ECT(1) new-data segments sent.
    pub sent_ect1: u64,
}

impl RefClassic {
    /// SYN-ACK / SYN input: a bare ECE negotiates classic ECN.
    pub fn input_syn_sent(&mut self, flags: u16, counters: &mut RefCounters) {
        // Mode 2 only raises the permit on a bare ECE; a later non-ECE SYN does not clear it.
        if flags & (TH_CWR | TH_ECE) == TH_ECE {
            self.permit = true;
            counters.handshakes = counters.handshakes.wrapping_add(1);
        }
    }

    /// Established input: count the codepoint, then apply CWR/CE when permitted.
    pub fn input_segment(&mut self, flags: u16, iptos: u8, counters: &mut RefCounters) {
        match iptos & ECN_MASK {
            ECN_CE => counters.received_ce = counters.received_ce.wrapping_add(1),
            ECN_ECT0 => counters.received_ect0 = counters.received_ect0.wrapping_add(1),
            ECN_ECT1 => counters.received_ect1 = counters.received_ect1.wrapping_add(1),
            _ => {}
        }
        if !self.permit {
            return;
        }
        if flags & TH_CWR != 0 {
            self.snd_ece = false;
        }
        if iptos & ECN_MASK == ECN_CE {
            self.snd_ece = true;
        }
    }

    /// Established output: ECT on new data, CWR once, ECE while CE persists.
    pub fn output(&mut self, newdata: bool, counters: &mut RefCounters) -> (u8, u16) {
        let ipecn = if newdata {
            if self.use_ect1 {
                counters.sent_ect1 = counters.sent_ect1.wrapping_add(1);
                ECN_ECT1
            } else {
                counters.sent_ect0 = counters.sent_ect0.wrapping_add(1);
                ECN_ECT0
            }
        } else {
            ECN_NOTECT
        };
        let mut flags = 0;
        if newdata && self.snd_cwr {
            flags |= TH_CWR;
            self.snd_cwr = false;
        }
        if self.snd_ece {
            flags |= TH_ECE;
        }
        (ipecn, flags)
    }
}
