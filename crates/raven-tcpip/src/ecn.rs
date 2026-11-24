// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD-aligned TCP ECN state (K270).
//!
//! Ported from FreeBSD `sys/netinet/tcp_ecn.c` at
//! `133344405f532bd54ff3629a77d4c4e062f01fdb`: the RFC 3168 handshake and established
//! input/output rules plus the Accurate ECN table. The module keeps the donor's pure decisions
//! and leaves the congestion-controller callback to the caller.

/// `TH_ECE` from FreeBSD `netinet/tcp.h`.
pub const TH_ECE: u16 = 0x40;
/// `TH_CWR` from FreeBSD `netinet/tcp.h`.
pub const TH_CWR: u16 = 0x80;
/// `TH_AE` from FreeBSD `netinet/tcp.h`.
pub const TH_AE: u16 = 0x100;
/// `TH_SYN` from FreeBSD `netinet/tcp.h`.
pub const TH_SYN: u16 = 0x02;
/// `TH_ACK` from FreeBSD `netinet/tcp.h`.
pub const TH_ACK: u16 = 0x10;

/// `IPTOS_ECN_MASK`.
pub const ECN_MASK: u8 = 0x03;
/// `IPTOS_ECN_CE`.
pub const ECN_CE: u8 = 0x03;
/// `IPTOS_ECN_ECT0`.
pub const ECN_ECT0: u8 = 0x02;
/// `IPTOS_ECN_ECT1`.
pub const ECN_ECT1: u8 = 0x01;
/// `IPTOS_ECN_NOTECT`.
pub const ECN_NOTECT: u8 = 0x00;

/// FreeBSD `tcp_do_ecn` modes: 0 off, 1 RFC3168, 2 RFC3168 default, 3/4 Accurate ECN.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EcnState {
    /// `V_tcp_do_ecn`.
    pub mode: u8,
    /// `V_tcp_ecn_maxretries`.
    pub max_retries: u8,
    /// `TF2_ECN_PERMIT`.
    pub permit: bool,
    /// `TF2_ACE_PERMIT`.
    pub ace_permit: bool,
    /// `TF2_ECN_SND_ECE`.
    pub snd_ece: bool,
    /// `TF2_ECN_SND_CWR`.
    pub snd_cwr: bool,
    /// `TF2_ECN_USE_ECT1`.
    pub use_ect1: bool,
    /// `t_rcep` Accurate ECN receive counter.
    pub rcep: u32,
    /// `t_scep` Accurate ECN send counter.
    pub scep: u32,
}

/// FreeBSD ECN counters used by the ported functions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EcnCounters {
    /// `tcps_ecn_shs`.
    pub handshakes: u64,
    /// `tcps_ecn_rcvce`.
    pub received_ce: u64,
    /// `tcps_ecn_rcvect0`.
    pub received_ect0: u64,
    /// `tcps_ecn_rcvect1`.
    pub received_ect1: u64,
    /// `tcps_ecn_sndect0`.
    pub sent_ect0: u64,
    /// `tcps_ecn_sndect1`.
    pub sent_ect1: u64,
}

impl EcnState {
    /// A connection with a FreeBSD `tcp_do_ecn` mode and retry limit.
    #[must_use]
    pub const fn new(mode: u8, max_retries: u8) -> Self {
        Self {
            mode,
            max_retries,
            permit: false,
            ace_permit: false,
            snd_ece: false,
            snd_cwr: false,
            use_ect1: false,
            rcep: 0,
            scep: 0,
        }
    }

    /// FreeBSD `tcp_ecn_input_syn_sent`.
    pub fn input_syn_sent(&mut self, thflags: u16, iptos: u8, counters: &mut EcnCounters) {
        match self.mode {
            0 => {}
            1 | 2 => {
                if thflags & (TH_CWR | TH_ECE) == TH_ECE {
                    self.permit = true;
                    self.ace_permit = false;
                    counters.handshakes = counters.handshakes.wrapping_add(1);
                }
            }
            3 | 4 => {
                match thflags & (TH_AE | TH_CWR | TH_ECE) {
                    value if value == TH_ECE => {
                        self.permit = true;
                        self.ace_permit = false;
                        counters.handshakes = counters.handshakes.wrapping_add(1);
                    }
                    TH_CWR => {
                        self.ace_permit = true;
                        self.permit = false;
                        self.scep = 5;
                        counters.handshakes = counters.handshakes.wrapping_add(1);
                    }
                    TH_AE => {
                        self.ace_permit = true;
                        self.permit = false;
                        self.scep = 5;
                        counters.handshakes = counters.handshakes.wrapping_add(1);
                    }
                    value if value == (TH_CWR | TH_ECE) => {
                        self.ace_permit = true;
                        self.permit = false;
                        self.scep = 5;
                        counters.handshakes = counters.handshakes.wrapping_add(1);
                    }
                    value if value == (TH_AE | TH_CWR) => {
                        self.ace_permit = true;
                        self.permit = false;
                        self.scep = 6;
                        counters.handshakes = counters.handshakes.wrapping_add(1);
                    }
                    _ => {
                        self.permit = false;
                        self.ace_permit = false;
                    }
                }
                self.rcep = match iptos & ECN_MASK {
                    ECN_NOTECT => 0b010,
                    ECN_ECT0 => 0b100,
                    ECN_ECT1 => 0b011,
                    ECN_CE => 0b110,
                    _ => 0,
                };
            }
            _ => {}
        }
    }

    /// FreeBSD `tcp_ecn_input_parallel_syn`.
    pub fn input_parallel_syn(&mut self, thflags: u16, iptos: u8, counters: &mut EcnCounters) {
        if thflags & TH_ACK != 0 {
            return;
        }
        match self.mode {
            0 => {}
            1 | 2 => {
                if thflags & (TH_CWR | TH_ECE) == (TH_CWR | TH_ECE) {
                    self.permit = true;
                    self.ace_permit = false;
                    self.snd_ece = true;
                    counters.handshakes = counters.handshakes.wrapping_add(1);
                }
            }
            3 | 4 => match thflags & (TH_AE | TH_CWR | TH_ECE) {
                value if value == (TH_CWR | TH_ECE) => {
                    self.permit = true;
                    self.ace_permit = false;
                    self.snd_ece = true;
                    counters.handshakes = counters.handshakes.wrapping_add(1);
                }
                value if value == (TH_AE | TH_CWR | TH_ECE) => {
                    self.ace_permit = true;
                    self.permit = false;
                    self.snd_ece = true;
                    counters.handshakes = counters.handshakes.wrapping_add(1);
                    self.rcep = match iptos & ECN_MASK {
                        ECN_NOTECT => 0b010,
                        ECN_ECT0 => 0b100,
                        ECN_ECT1 => 0b011,
                        ECN_CE => 0b110,
                        _ => 0,
                    };
                }
                _ => {
                    self.permit = false;
                    self.ace_permit = false;
                }
            },
            _ => {}
        }
    }

    /// FreeBSD `tcp_ecn_input_segment`. Returns the Accurate ECN delta (`delta_cep`).
    pub fn input_segment(
        &mut self,
        thflags: u16,
        tlen: u32,
        pkts: u32,
        iptos: u8,
        counters: &mut EcnCounters,
    ) -> u32 {
        let _ = tlen;
        match iptos & ECN_MASK {
            ECN_CE => counters.received_ce = counters.received_ce.wrapping_add(1),
            ECN_ECT0 => counters.received_ect0 = counters.received_ect0.wrapping_add(1),
            ECN_ECT1 => counters.received_ect1 = counters.received_ect1.wrapping_add(1),
            _ => {}
        }
        if !(self.permit || self.ace_permit) {
            return 0;
        }
        let mut delta_cep = 0;
        if self.ace_permit {
            if iptos & ECN_MASK == ECN_CE {
                self.rcep = self.rcep.wrapping_add(1);
            }
            if self.permit {
                delta_cep = (get_ace(thflags) + 8 - (self.scep & 7)) & 7;
                if delta_cep < pkts {
                    delta_cep = pkts - ((pkts - delta_cep) & 7);
                }
                self.scep = self.scep.wrapping_add(delta_cep);
            } else {
                match get_ace(thflags) {
                    0b010..=0b100 => self.scep = 5,
                    0b110 => self.scep = 6,
                    _ => self.scep = 5,
                }
                self.permit = true;
            }
        } else {
            if thflags & (TH_SYN | TH_ECE) == TH_ECE {
                delta_cep = 1;
                self.scep = self.scep.wrapping_add(1);
            }
            if thflags & TH_CWR != 0 {
                self.snd_ece = false;
            }
            if iptos & ECN_MASK == ECN_CE {
                self.snd_ece = true;
            }
        }
        delta_cep
    }

    /// FreeBSD `tcp_ecn_output_syn_sent`.
    #[must_use]
    pub fn output_syn_sent(&self, rxtshift: u32) -> u16 {
        match self.mode {
            0 => 0,
            1 => {
                if rxtshift >= 1 {
                    if rxtshift <= u32::from(self.max_retries) {
                        TH_ECE | TH_CWR
                    } else {
                        0
                    }
                } else {
                    TH_ECE | TH_CWR
                }
            }
            3 => {
                if rxtshift >= 1 {
                    if rxtshift <= u32::from(self.max_retries) {
                        TH_ECE | TH_CWR | TH_AE
                    } else {
                        0
                    }
                } else {
                    TH_ECE | TH_CWR | TH_AE
                }
            }
            _ => 0,
        }
    }

    /// FreeBSD `tcp_ecn_output_established`.
    ///
    /// Returns the IP ECN codepoint (`ECN_NOTECT`, `ECN_ECT0` or `ECN_ECT1`) and updates the
    /// supplied TCP flags with CWR/ECE.
    #[allow(clippy::too_many_arguments)] // Mirrors the FreeBSD tcp_ecn_output_established inputs.
    pub fn output_established(
        &mut self,
        thflags: &mut u16,
        len: u32,
        snd_nxt: u32,
        snd_max: u32,
        rxmit: bool,
        forcedata: bool,
        counters: &mut EcnCounters,
    ) -> u8 {
        let newdata = len > 0 && snd_nxt >= snd_max && !rxmit && !(forcedata && len == 1);
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
        if self.ace_permit {
            set_ace(thflags, self.rcep);
            if !self.permit {
                self.rcep = if self.rcep == 0b110 { 6 } else { 5 };
                self.permit = true;
            }
        } else {
            if newdata && self.snd_cwr {
                *thflags |= TH_CWR;
                self.snd_cwr = false;
            }
            if self.snd_ece {
                *thflags |= TH_ECE;
            }
        }
        ipecn
    }
}

/// FreeBSD `tcp_ecn_get_ace`.
#[must_use]
pub const fn get_ace(thflags: u16) -> u32 {
    ((thflags & (TH_AE | TH_CWR | TH_ECE)) >> 6) as u32
}

/// FreeBSD `tcp_ecn_set_ace`.
pub fn set_ace(thflags: &mut u16, rcep: u32) {
    *thflags &= !(TH_AE | TH_CWR | TH_ECE);
    *thflags |= ((rcep as u16) << 6) & (TH_AE | TH_CWR | TH_ECE);
}

#[cfg(test)]
#[path = "ecn_ut.rs"]
mod tests;
