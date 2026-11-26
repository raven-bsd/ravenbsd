// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD `tcp_maxseg`/`tcp_fixed_maxseg` and the peer-MSS clamp.
//!
//! This is a sans-IO port of `sys/netinet/tcp_subr.c:tcp_maxseg` and
//! `tcp_fixed_maxseg`, plus the peer-option part of
//! `sys/netinet/tcp_input.c:tcp_mss_update`. It does not read a `tcpcb` and it
//! does not wire into the stack; the caller supplies the negotiated MSS and the
//! option flags.
//!
//! `tcp_maxseg` subtracts every option that consumes payload space, including
//! the variable SACK blocks. `tcp_fixed_maxseg` subtracts only the options that
//! are sent on every segment, so the result is stable for congestion-control
//! modulo arithmetic.

use crate::pmtu::{IpFamily, TCP_MINMSS, TCP_MSS_DEFAULT, mss_update};

/// FreeBSD `TCP6_MSS`: the IPv6 default MSS when no peer option is present.
pub const TCP6_MSS_DEFAULT: u32 = 1220;

/// FreeBSD `TCP_MAXOLEN`: the maximum option space in a TCP header.
pub const TCP_MAXOLEN: u32 = 40;

const TCPOLEN_MAXSEG: u32 = 4;
const TCPOLEN_WINDOW: u32 = 3;
const TCPOLEN_SACK_PERMITTED: u32 = 2;
const TCPOLEN_SACKHDR: u32 = 2;
const TCPOLEN_SACK: u32 = 8;
const TCPOLEN_TSTAMP_APPA: u32 = 12;
const TCPOLEN_SIGNATURE: u32 = 18;

/// Whether the connection has completed the handshake.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MssState {
    /// SYN or SYN-ACK: only the options sent on the handshake are present.
    Handshake,
    /// `TCPS_HAVEESTABLISHED`: timestamps and variable SACK blocks may apply.
    Established,
}

/// The negotiated TCP options that consume MSS space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MssOptions {
    /// `TF_NOOPT`: no options are sent, so no option space is subtracted.
    pub no_options: bool,
    /// Established state and `TF_RCVD_TSTMP`: the 12-byte timestamp appendix.
    pub received_timestamp: bool,
    /// Handshake state and `TF_REQ_TSTMP`.
    pub requested_timestamp: bool,
    /// Handshake state and `TF_REQ_SCALE`.
    pub requested_window_scale: bool,
    /// `TF_SACK_PERMIT`.
    pub sack_permitted: bool,
    /// `TF_SIGNATURE` (TCP MD5/AO).
    pub signature: bool,
    /// `rcv_numsacks`: SACK blocks sent on this segment (0..=6 in FreeBSD).
    pub received_sack_blocks: u8,
}

impl MssOptions {
    /// No options are negotiated.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            no_options: true,
            received_timestamp: false,
            requested_timestamp: false,
            requested_window_scale: false,
            sack_permitted: false,
            signature: false,
            received_sack_blocks: 0,
        }
    }
}

/// FreeBSD `PADTCPOLEN`: round an option length up to a four-byte boundary.
#[must_use]
pub const fn pad_option_length(len: u32) -> u32 {
    ((len / 4) + ((!len.is_multiple_of(4)) as u32)) * 4
}

/// The option bytes that `tcp_maxseg` subtracts.
#[must_use]
pub fn option_length(state: MssState, options: &MssOptions) -> u32 {
    if options.no_options {
        return 0;
    }
    let mut optlen = match state {
        MssState::Established => {
            if options.received_timestamp {
                TCPOLEN_TSTAMP_APPA
            } else {
                0
            }
        }
        MssState::Handshake => {
            if options.requested_timestamp {
                TCPOLEN_TSTAMP_APPA
            } else {
                pad_option_length(TCPOLEN_MAXSEG)
            }
        }
    };
    match state {
        MssState::Established => {
            if options.signature {
                optlen += pad_option_length(TCPOLEN_SIGNATURE);
            }
            if options.sack_permitted && options.received_sack_blocks > 0 {
                optlen += TCPOLEN_SACKHDR;
                optlen += u32::from(options.received_sack_blocks) * TCPOLEN_SACK;
                optlen = pad_option_length(optlen);
            }
        }
        MssState::Handshake => {
            if options.requested_window_scale {
                optlen += pad_option_length(TCPOLEN_WINDOW);
            }
            if options.signature {
                optlen += pad_option_length(TCPOLEN_SIGNATURE);
            }
            if options.sack_permitted {
                optlen += pad_option_length(TCPOLEN_SACK_PERMITTED);
            }
        }
    }
    optlen.min(TCP_MAXOLEN)
}

/// FreeBSD `tcp_maxseg`: the effective SMSS including variable SACK blocks.
#[must_use]
pub fn tcp_maxseg(maxseg: u32, state: MssState, options: &MssOptions) -> u32 {
    if options.no_options {
        return maxseg;
    }
    maxseg.saturating_sub(option_length(state, options))
}

/// FreeBSD `tcp_fixed_maxseg`: effective SMSS without variable SACK blocks.
#[must_use]
pub fn tcp_fixed_maxseg(maxseg: u32, state: MssState, options: &MssOptions) -> u32 {
    if options.no_options {
        return maxseg;
    }
    let mut fixed = *options;
    fixed.received_sack_blocks = 0;
    maxseg.saturating_sub(option_length(state, &fixed))
}

/// Peer MSS from the path MTU and the peer's MSS option.
///
/// `peer_mss == None` means the SYN carried no MSS option, matching FreeBSD's
/// `offer == 0` arm. A present offer is rounded up to [`TCP_MINMSS`] and the
/// result is bounded by the path MTU and the 64-byte floor.
#[must_use]
pub fn mss_from_peer(family: IpFamily, mtu: u32, peer_mss: Option<u16>) -> u32 {
    let default = match family {
        IpFamily::V4 => TCP_MSS_DEFAULT,
        IpFamily::V6 => TCP6_MSS_DEFAULT,
    };
    let offer = peer_mss.map_or(0, i32::from);
    mss_update(
        offer,
        u32::MAX,
        mtu,
        0,
        default,
        TCP_MINMSS,
        true,
        true,
        u32::MAX,
        family,
    )
    .maxseg
}

#[cfg(test)]
#[path = "tcp_mss_ut.rs"]
mod tests;
