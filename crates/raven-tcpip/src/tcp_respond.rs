// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD sys/netinet/tcp_subr.c, revision
// 133344405f532bd54ff3629a77d4c4e062f01fdb: tcp_respond, tcp_drop, tcp_close and
// tcp_discardcb. The upstream notice follows.
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

//! Sans-IO TCP reset, response and teardown helpers.
//!
//! This is the host-testable model of FreeBSD `tcp_respond`, `tcp_drop`, `tcp_close` and
//! `tcp_discardcb`. It computes the response fields, pseudo-header checksum input, TTL and
//! counter outcomes; it does not allocate mbufs, send frames or touch the stack state machine.

use raven_abi::errno::Errno;
use raven_tcpip_wire::tcp::TcpFlags;

use crate::tcp_state::TcpState;
use crate::{IpAddr, Ipv4Addr, Ipv6Addr};

/// FreeBSD `V_ip_defttl`.
pub const IP_DEFTTL: u8 = 64;
/// FreeBSD `TCP_MAXWIN`.
pub const TCP_MAXWIN: u32 = 65_535;

/// One address and port.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endpoint {
    /// Address.
    pub address: IpAddr,
    /// Port.
    pub port: u16,
}

/// The header fields of the segment that triggered the response.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TcpHeader {
    /// Source port.
    pub source_port: u16,
    /// Destination port.
    pub destination_port: u16,
    /// Sequence number.
    pub sequence: u32,
    /// Acknowledgement number.
    pub acknowledgement: u32,
    /// Control bits.
    pub flags: TcpFlags,
    /// Advertised window.
    pub window: u16,
    /// TCP payload length, used by the RST acknowledgement rule.
    pub payload_len: u32,
}

/// Connection facts the donor reads from `tcpcb`/`inpcb`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ConnectionInfo {
    /// Free bytes in the receive buffer (`sbspace`).
    pub receive_space: u32,
    /// Receive window scale.
    pub receive_scale: u8,
    /// IP TTL or IPv6 hop limit (`inp_ip_ttl` / `in6_selecthlim`).
    pub ip_ttl: u8,
    /// `TF_NOOPT`: do not include options.
    pub no_options: bool,
    /// Option bytes to include, excluding padding. Zero means a 20-byte header.
    pub option_len: u8,
}

/// Whether the response comes from a keepalive template or replies to a received segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RespondMode {
    /// `m == NULL`: copy the template and force `ACK`.
    Template,
    /// `m != NULL`: reply with the caller's flags.
    Reply,
}

/// One `tcp_respond` request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RespondRequest<'a> {
    /// Local endpoint of the connection.
    pub local: Endpoint,
    /// Remote endpoint of the connection.
    pub remote: Endpoint,
    /// Header of the received segment or template.
    pub incoming: &'a TcpHeader,
    /// Response mode.
    pub mode: RespondMode,
    /// Connection facts, or `None` when the response has no control block.
    pub connection: Option<ConnectionInfo>,
}

/// Pseudo-header input for the transport checksum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChecksumInput {
    /// IPv4 pseudo-header input.
    V4 {
        /// Source address.
        source: Ipv4Addr,
        /// Destination address.
        destination: Ipv4Addr,
        /// TCP length (header plus payload).
        length: u16,
    },
    /// IPv6 pseudo-header input.
    V6 {
        /// Source address.
        source: Ipv6Addr,
        /// Destination address.
        destination: Ipv6Addr,
        /// TCP length (header plus payload).
        length: u32,
    },
}

/// Counter increments from one response.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RespondCounters {
    /// `tcps_sndacks`.
    pub send_acks: u64,
    /// `tcps_sndctrl`.
    pub send_control: u64,
    /// `tcps_sndtotal`.
    pub send_total: u64,
}

/// The response fields the caller turns into a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RespondResult {
    /// Response source endpoint.
    pub source: Endpoint,
    /// Response destination endpoint.
    pub destination: Endpoint,
    /// TCP sequence number.
    pub sequence: u32,
    /// TCP acknowledgement number.
    pub acknowledgement: u32,
    /// Control bits.
    pub flags: TcpFlags,
    /// Advertised window.
    pub window: u16,
    /// IPv4 TTL or IPv6 hop limit.
    pub ttl: u8,
    /// Pseudo-header input for the checksum.
    pub checksum: ChecksumInput,
    /// Counter increments.
    pub counters: RespondCounters,
    /// Whether a reset must be logged (`tcp_log_end_status`).
    pub reset_logged: bool,
    /// Whether option bytes belong in the header.
    pub options_included: bool,
}

/// Why a response cannot be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RespondError {
    /// The local and remote addresses are different families.
    AddressFamilyMismatch,
    /// The option length is outside `0..=40` or not word aligned.
    BadOptionLength,
}

/// RST / RST|ACK selection for a segment that must be refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ResetSelection {
    /// Control bits.
    pub flags: TcpFlags,
    /// Sequence number.
    pub sequence: u32,
    /// Acknowledgement number.
    pub acknowledgement: u32,
}

/// One `tcp_drop` request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropRequest {
    /// Whether the connection has received a SYN (`TCPS_HAVERCVDSYN`).
    pub synchronized: bool,
    /// Error reported to the socket.
    pub error: Errno,
    /// `t_softerror`, used when the hard error is `ETIMEDOUT`.
    pub soft_error: Option<Errno>,
}

/// Counter increments from `tcp_drop` and `tcp_close`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DropCounters {
    /// `tcps_drops`.
    pub drops: u64,
    /// `tcps_conndrops`.
    pub connection_drops: u64,
    /// `tcps_closed`.
    pub closed: u64,
}

/// The outcome of `tcp_drop`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropOutcome {
    /// Whether the donor calls `tcp_output_nodrop` to send a RST.
    pub reset_required: bool,
    /// Effective socket error after the `t_softerror` substitution.
    pub error: Errno,
    /// Counter increments.
    pub counters: DropCounters,
}

/// One `tcp_close` request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseRequest {
    /// State before the close.
    pub state: TcpState,
}

/// Counter increments from `tcp_close`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloseCounters {
    /// `tcps_closed`.
    pub closed: u64,
}

/// The outcome of `tcp_close`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseOutcome {
    /// State after the close.
    pub state: TcpState,
    /// Whether `tcp_state_change` ran.
    pub state_changed: bool,
    /// Whether `soisdisconnected` ran.
    pub disconnected: bool,
    /// Counter increments.
    pub counters: CloseCounters,
}

/// One `tcp_discardcb` request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscardRequest {
    /// `t_rttupdated`.
    pub rtt_samples: u8,
    /// Frames still queued on `t_inqueue`.
    pub inqueue_frames: u32,
    /// `snd_ssthresh`.
    pub snd_ssthresh: u32,
    /// `t_maxseg`.
    pub maxseg: u16,
    /// `so_snd.sb_hiwat`.
    pub send_high_water: u32,
    /// Whether the connection is IPv6, for the header size in the RTT update.
    pub ipv6: bool,
}

/// The outcome of `tcp_discardcb`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiscardOutcome {
    /// Whether the reassembly queue was flushed.
    pub reassembly_flushed: bool,
    /// Whether the congestion-control state was detached.
    pub cc_detached: bool,
    /// Frames freed from the input queue.
    pub inqueue_dropped: u32,
    /// Whether the hostcache RTT update ran.
    pub rtt_saved: bool,
    /// The RTT-updated slow-start threshold, or `None` when no update ran.
    pub ssthresh: Option<u32>,
}

/// FreeBSD `tcp_respond`: the scaled receive window, zero for a reset.
fn response_window(connection: Option<ConnectionInfo>, flags: TcpFlags) -> u16 {
    let Some(info) = connection else {
        return 0;
    };
    if flags.rst() {
        return 0;
    }
    let scale = info.receive_scale.min(14);
    let limit = TCP_MAXWIN << scale;
    let space = info.receive_space.min(limit);
    u16::try_from(space >> scale).unwrap_or(u16::MAX)
}

/// The option bytes the response header carries.
fn response_options(connection: Option<ConnectionInfo>) -> Result<(bool, u16), RespondError> {
    let Some(info) = connection else {
        return Ok((false, 0));
    };
    if info.no_options {
        return Ok((false, 0));
    }
    if info.option_len > 40 || info.option_len % 4 != 0 {
        return Err(RespondError::BadOptionLength);
    }
    Ok((true, u16::from(info.option_len)))
}

/// Build the response fields for one TCP segment.
///
/// # Errors
///
/// Returns [`RespondError`] for a mixed address family or an invalid option length.
pub fn tcp_respond(
    request: &RespondRequest<'_>,
    ack: u32,
    seq: u32,
    flags: TcpFlags,
) -> Result<RespondResult, RespondError> {
    let (options_included, option_len) = response_options(request.connection)?;
    let header_len = 20u16
        .checked_add(option_len)
        .ok_or(RespondError::BadOptionLength)?;
    let effective_flags = match request.mode {
        RespondMode::Template => TcpFlags::ACK,
        RespondMode::Reply => flags,
    };
    let checksum = match (request.local.address, request.remote.address) {
        (IpAddr::V4(source), IpAddr::V4(destination)) => ChecksumInput::V4 {
            source,
            destination,
            length: header_len,
        },
        (IpAddr::V6(source), IpAddr::V6(destination)) => ChecksumInput::V6 {
            source,
            destination,
            length: u32::from(header_len),
        },
        _ => return Err(RespondError::AddressFamilyMismatch),
    };
    let mut counters = RespondCounters {
        send_total: 1,
        ..RespondCounters::default()
    };
    if effective_flags.ack() {
        counters.send_acks = 1;
    } else if effective_flags.syn() || effective_flags.fin() || effective_flags.rst() {
        counters.send_control = 1;
    }
    Ok(RespondResult {
        source: request.local,
        destination: request.remote,
        sequence: seq,
        acknowledgement: ack,
        flags: effective_flags,
        window: response_window(request.connection, effective_flags),
        ttl: request
            .connection
            .map_or(IP_DEFTTL, |connection| connection.ip_ttl),
        checksum,
        counters,
        reset_logged: request.connection.is_some() && effective_flags.rst(),
        options_included,
    })
}

/// Select RST versus RST|ACK and the sequence/acknowledgement numbers.
///
/// This is the FreeBSD `tcp_dropwithreset` rule: an ACKed segment gets a bare RST echoing its
/// acknowledgement, a segment without ACK gets RST|ACK acknowledging its sequence space
/// (`th_seq + tlen + SYN + FIN`).
#[must_use]
pub fn reset_selection(incoming: &TcpHeader) -> ResetSelection {
    if incoming.flags.ack() {
        return ResetSelection {
            flags: TcpFlags::RST,
            sequence: incoming.acknowledgement,
            acknowledgement: 0,
        };
    }
    let mut acknowledged = incoming.payload_len;
    if incoming.flags.syn() {
        acknowledged = acknowledged.saturating_add(1);
    }
    if incoming.flags.fin() {
        acknowledged = acknowledged.saturating_add(1);
    }
    ResetSelection {
        flags: TcpFlags::from_bits(TcpFlags::RST.bits() | TcpFlags::ACK.bits()),
        sequence: 0,
        acknowledgement: incoming.sequence.wrapping_add(acknowledged),
    }
}

/// Apply the donor's drop path.
#[must_use]
pub fn tcp_drop(request: &DropRequest) -> DropOutcome {
    let error = if request.error == Errno::ETIMEDOUT {
        request.soft_error.unwrap_or(request.error)
    } else {
        request.error
    };
    let (drops, connection_drops) = if request.synchronized { (1, 0) } else { (0, 1) };
    DropOutcome {
        reset_required: request.synchronized,
        error,
        counters: DropCounters {
            drops,
            connection_drops,
            closed: 1,
        },
    }
}

/// Apply the donor's close path.
#[must_use]
pub fn tcp_close(request: &CloseRequest) -> CloseOutcome {
    CloseOutcome {
        state: TcpState::Closed,
        state_changed: request.state != TcpState::Closed,
        disconnected: true,
        counters: CloseCounters { closed: 1 },
    }
}

/// Apply the donor's control-block discard path.
#[must_use]
pub fn tcp_discardcb(request: &DiscardRequest) -> DiscardOutcome {
    let rtt_saved = request.rtt_samples >= 4;
    let ssthresh = if rtt_saved {
        let threshold = u64::from(request.snd_ssthresh);
        let high_water = u64::from(request.send_high_water);
        let maxseg = u64::from(request.maxseg.max(1));
        if threshold != 0 && threshold < high_water / 2 {
            let packets = ((threshold + maxseg / 2) / maxseg).max(2);
            let header = if request.ipv6 { 60 } else { 40 };
            let bytes = packets.saturating_mul(maxseg.saturating_add(header));
            Some(u32::try_from(bytes).unwrap_or(u32::MAX))
        } else {
            Some(0)
        }
    } else {
        None
    };
    DiscardOutcome {
        reassembly_flushed: true,
        cc_detached: true,
        inqueue_dropped: request.inqueue_frames,
        rtt_saved,
        ssthresh,
    }
}

#[cfg(test)]
#[path = "tcp_respond_ut.rs"]
mod tests;
