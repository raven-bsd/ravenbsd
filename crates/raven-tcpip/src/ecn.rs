// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! NetBSD-aligned TCP ECN state (K259).
//!
//! This is the literal Rust port of the ECN decisions in NetBSD
//! `sys/netinet/tcp_input.c`, `tcp_output.c`, `tcp_subr.c` and the `TF_ECN_*` flags in
//! `tcp_var.h`. The handshake, CE/CWR handling and output marking are separate from the
//! congestion controller so the existing recovery path remains unchanged.

/// `IPTOS_ECN_MASK` from NetBSD's IP header.
pub const ECN_MASK: u8 = 0x03;
/// `IPTOS_ECN_CE`.
pub const ECN_CE: u8 = 0x03;
/// `IPTOS_ECN_ECT0`.
pub const ECN_ECT0: u8 = 0x02;
/// `IPTOS_ECN_ECT1`.
pub const ECN_ECT1: u8 = 0x01;

/// NetBSD `t_ecn_retries` / `TF_ECN_PERMIT` / `TF_ECN_SND_CWR` / `TF_ECN_SND_ECE`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EcnState {
    /// `tcp_do_ecn` at connection setup.
    pub enabled: bool,
    /// `TF_ECN_PERMIT`: the peer completed the ECN handshake.
    pub permit: bool,
    /// `TF_ECN_SND_CWR`: a CWR is queued for the next output.
    pub snd_cwr: bool,
    /// `TF_ECN_SND_ECE`: an ECE is queued for the next output.
    pub snd_ece: bool,
    /// Remaining SYN ECN setup retries (`t_ecn_retries`).
    pub retries: u8,
}

/// NetBSD TCP ECN counters used by the three relevant paths.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EcnCounters {
    /// `TCP_STAT_ECN_SHS`.
    pub handshakes: u64,
    /// `TCP_STAT_ECN_CE`.
    pub ce: u64,
    /// `TCP_STAT_ECN_ECT`.
    pub ect: u64,
}

/// The ECN bits for one outgoing packet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EcnOutput {
    /// Mark the IP header ECT(0).
    pub ect0: bool,
    /// Set TH_CWR.
    pub cwr: bool,
    /// Set TH_ECE.
    pub ece: bool,
}

impl EcnState {
    /// A connection with `tcp_do_ecn` state.
    #[must_use]
    pub const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            permit: false,
            snd_cwr: false,
            snd_ece: false,
            retries: 0,
        }
    }

    /// ECN SYN/SYN-ACK setup from `tcp_output.c`.
    ///
    /// A first SYN sets ECE|CWR and arms `max_retries`; a retransmitted SYN sets the bits while
    /// retries remain. The return value is whether the outgoing flags carry ECE|CWR.
    pub fn on_syn_sent(&mut self, retransmit: bool, max_retries: u8) -> bool {
        if !self.enabled {
            return false;
        }
        if retransmit {
            if self.retries == 0 {
                return false;
            }
            self.retries -= 1;
            true
        } else {
            self.retries = max_retries;
            true
        }
    }

    /// `TCPS_SYN_SENT` ACK handling from `tcp_input.c`.
    ///
    /// The peer's ECE and the local `tcp_do_ecn` both have to be true to set `TF_ECN_PERMIT`.
    pub fn on_syn_ack(&mut self, ece: bool, ecn_enabled: bool, counters: &mut EcnCounters) -> bool {
        self.permit = ece && ecn_enabled;
        if self.permit {
            counters.handshakes = counters.handshakes.wrapping_add(1);
        }
        self.permit
    }

    /// `tcp_input.c` ECN block for an established segment.
    ///
    /// It clears a queued ECE on CWR, records CE/ECT for the counters, and reports whether the
    /// ECE bit is a new congestion event. The caller still applies its normal recovery gate.
    pub fn on_segment(
        &mut self,
        cwr: bool,
        ece: bool,
        tos_ecn: u8,
        snd_una: u32,
        snd_recover: u32,
        counters: &mut EcnCounters,
    ) -> bool {
        if !self.permit {
            return false;
        }
        if cwr {
            self.snd_ece = false;
        }
        match tos_ecn & ECN_MASK {
            ECN_CE => {
                self.snd_ece = true;
                counters.ce = counters.ce.wrapping_add(1);
            }
            ECN_ECT0 => {
                counters.ect = counters.ect.wrapping_add(1);
            }
            ECN_ECT1 => {}
            _ => {}
        }
        ece && snd_una >= snd_recover
    }

    /// `tcp_output.c` ECN marking for one segment.
    ///
    /// Pure ACKs, retransmissions and a one-byte persist probe are not marked ECT(0). A queued
    /// CWR is consumed by this output; a queued ECE remains until the peer sends CWR.
    pub fn on_output(
        &mut self,
        len: u32,
        snd_nxt: u32,
        snd_max: u32,
        force_window_probe: bool,
        counters: &mut EcnCounters,
    ) -> EcnOutput {
        if !self.permit {
            return EcnOutput::default();
        }
        let ect0 = len > 0 && snd_nxt >= snd_max && !(force_window_probe && len == 1);
        if ect0 {
            counters.ect = counters.ect.wrapping_add(1);
        }
        let cwr = self.snd_cwr;
        if cwr {
            self.snd_cwr = false;
        }
        EcnOutput {
            ect0,
            cwr,
            ece: self.snd_ece,
        }
    }
}

#[cfg(test)]
#[path = "ecn_ut.rs"]
mod tests;
