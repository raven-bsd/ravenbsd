// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TCP connection state machine, RFC 5961 hardening and lifecycle timers.

use alloc::boxed::Box;
use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader, TcpOption, TcpPacket};

use crate::device::Device;
use crate::ipv4_stack::to_wire as to_wire4;
use crate::ipv6_stack::{IpAddr, Ipv6Addr, to_wire as to_wire6};
use crate::isn::{ISN_SECRET_LENGTH, IsnConn, IsnEntropy};
use crate::rto::RtoEstimator;
use crate::seq::{seq_ge, seq_gt, seq_le, seq_lt};
use crate::socket::Kind;
use crate::syncache::{
    AbortOutcome, AckRequest, Address as SyncAddress, Endpoint as SyncEndpoint, InsertOutcome,
    Key as SyncKey, PromoteOutcome, SynOptions, SynRequest,
};
use crate::tcp_data::advertised_window;
use crate::{Instant, Ipv4Addr, SocketId, Stack};

type Retransmit = (Vec<u8>, IpAddr, (IpAddr, u16), u16, u32, u32, u16);

/// The eleven TCP states from RFC 9293.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TcpState {
    /// No connection.
    Closed,
    /// Passive open waiting for a SYN.
    Listen,
    /// Active open SYN sent.
    SynSent,
    /// SYN received, SYN-ACK sent.
    SynReceived,
    /// Data transfer possible.
    Established,
    /// Our FIN sent, waiting for its ACK.
    FinWait1,
    /// Our FIN acknowledged.
    FinWait2,
    /// Peer FIN received, waiting for the application to close.
    CloseWait,
    /// Both sides sent FIN, our FIN not yet acknowledged.
    Closing,
    /// Peer FIN received, our FIN sent, waiting for its ACK.
    LastAck,
    /// TIME-WAIT, keeping the four-tuple bound.
    TimeWait,
}

pub(crate) struct TcpConnection {
    pub state: TcpState,
    pub local: IpAddr,
    pub local_port: u16,
    pub remote: Option<(IpAddr, u16)>,
    pub iss: u32,
    pub irs: u32,
    pub snd_una: u32,
    pub snd_nxt: u32,
    pub snd_wnd: u32,
    pub max_snd_wnd: u16,
    pub rcv_nxt: u32,
    pub rcv_wnd: u16,
    #[allow(dead_code)] // Window-update ordering is consumed by C09's send path.
    pub snd_wl1: u32,
    #[allow(dead_code)]
    pub snd_wl2: u32,
    /// Sequence number of the last ACK sent; FreeBSD RST validation uses it.
    pub last_ack_sent: u32,
    pub fin_sent: bool,
    pub fin_acked: bool,
    pub peer_fin: bool,
    pub close_requested: bool,
    pub passive: bool,
    pub connect_deadline: Option<Instant>,
    pub connect_retries: u8,
    pub time_wait_deadline: Option<Instant>,
    pub fin_wait2_deadline: Option<Instant>,
    pub keepalive_deadline: Option<Instant>,
    pub keepalive_interval_ms: u64,
    pub keepalive_probe_interval_ms: u64,
    pub keepalive_probes: u8,
    pub keepalive_left: u8,
    pub challenge_tokens: u32,
    pub challenge_last: Instant,
    pub mss: u16,
    pub nodelay: bool,
    pub ack_deadline: Option<Instant>,
    pub ack_segments: u8,
    pub ooo: Vec<(u32, Vec<u8>)>,
    pub ooo_limit: usize,
    pub zero_window_deadline: Option<Instant>,
    pub zero_window_backoff_ms: u64,
    pub zero_window_probes: usize,
    pub urgent_position: Option<u16>,
    pub urgent_count: usize,
    pub rto: RtoEstimator,
    pub rto_deadline: Option<Instant>,
    pub rtt_sample: Option<(u32, Instant, bool)>,
    pub retransmit_count: u8,
    pub rto_timeouts: usize,
    pub last_retransmit_seq: Option<u32>,
    pub rto_max_retransmits: u8,
    pub snd_wscale: u8,
    pub rcv_wscale: u8,
    pub offer_wscale: bool,
    pub offer_timestamps: bool,
    pub offer_sack: bool,
    pub timestamps: bool,
    pub ts_recent: u32,
    pub ts_recent_age: Instant,
    #[allow(dead_code)] // Read by the testutil option-inspection hook.
    pub last_tsval: u32,
    pub last_ts_sent: Option<(u32, Instant)>,
    pub peer_mss: u16,
    pub our_mss: u16,
    pub mss_clamps: usize,
    pub mss_floor: u16,
    pub paws_drops: usize,
    pub sack_permitted: bool,
    pub negotiated_wscale: Option<u8>,
    pub ecn_enabled: bool,
    pub ecn_negotiated: bool,
    pub ecn_cwr_pending: bool,
    /// Receiver saw CE and must echo ECE on its next ACKs.
    pub ecn_ece_pending: bool,
    pub retransmits_total: u32,
    pub dupack_count: u8,
    pub last_activity: Instant,
    pub last_adv_window: u16,
    pub last_ack_window: u16,
    pub cc: Box<dyn crate::cc::CongestionControl>,
    pub rack: crate::recovery::Rack,
    pub rack_sent: Vec<(u32, u32)>,
    pub tlp: crate::recovery::Tlp,
    pub tlp_enabled: bool,
    pub tlp_probe_at: Option<Instant>,
    pub ecn: crate::recovery::Ecn,
    pub recover_seq: u32,
    /// Absolute SACKed sequence ranges above `snd_una`, sorted and merged.
    pub sacked: Vec<(u32, u32)>,
    /// One past the highest sequence retransmitted in the current recovery.
    pub high_rxt: u32,
    /// Loss recovery (fast retransmit or RTO) is in progress.
    pub sack_recovery: bool,
    /// The recovery started with an RTO, so every hole below a SACK is lost.
    pub rto_recovery: bool,
    /// The largest segment payload the interface and peer allow; the PLPMTUD ceiling.
    pub plp_ceiling: u16,
    /// The PLPMTUD search, once started.
    pub plp: Option<crate::pmtu::Plpmtud>,
    /// Whether the search decision was taken for this connection.
    pub plp_decided: bool,
    /// An in-flight probe: end sequence and payload size.
    pub plp_probe: Option<(u32, u32)>,
    /// Earliest time of the next probe, in milliseconds.
    pub plp_next_ms: i64,
    /// Consecutive losses of the probe size under test.
    pub plp_attempts: u8,
    /// Window state from before the fast-retransmit response, for a spurious-retransmit undo.
    pub undo: Option<crate::recovery::SpuriousUndo>,
    /// Millisecond clock at the fast retransmit, compared with the echoed timestamp.
    pub recovery_ts: u32,
    /// Segments of reordering the loss detector tolerates; grows when reordering is observed.
    pub reorder_dist: u32,
    /// Highest sequence ever sent, kept only while `snd_nxt` is rewound for a go-back resend.
    pub snd_max: Option<u32>,
}

impl TcpConnection {
    /// The highest sequence number sent so far, even after `snd_nxt` was rewound to resend.
    pub(crate) fn highest_sent(&self) -> u32 {
        match self.snd_max {
            Some(max) if seq_gt(max, self.snd_nxt) => max,
            _ => self.snd_nxt,
        }
    }

    /// Rewind `snd_nxt` to `snd_una` so the unacknowledged data is sent again as new data.
    pub(crate) fn rewind_for_resend(&mut self) {
        self.snd_max = Some(self.highest_sent());
        self.snd_nxt = self.snd_una;
    }
}

impl TcpConnection {
    pub(crate) fn new_active(
        local: IpAddr,
        local_port: u16,
        remote: (IpAddr, u16),
        iss: u32,
        now: Instant,
    ) -> Self {
        Self {
            state: TcpState::SynSent,
            local,
            local_port,
            remote: Some(remote),
            iss,
            irs: 0,
            snd_una: iss,
            snd_nxt: iss.wrapping_add(1),
            snd_wnd: 0,
            max_snd_wnd: 0,
            rcv_nxt: 0,
            rcv_wnd: 65_535,
            snd_wl1: 0,
            snd_wl2: 0,
            last_ack_sent: iss,
            fin_sent: false,
            fin_acked: false,
            peer_fin: false,
            close_requested: false,
            passive: false,
            connect_deadline: Some(Instant::from_millis(now.as_millis().saturating_add(1000))),
            connect_retries: 0,
            time_wait_deadline: None,
            fin_wait2_deadline: None,
            keepalive_deadline: None,
            keepalive_interval_ms: 7_200_000,
            keepalive_probe_interval_ms: 75_000,
            keepalive_probes: 9,
            keepalive_left: 9,
            challenge_tokens: 10,
            challenge_last: now,
            mss: 1460,
            nodelay: false,
            ack_deadline: None,
            ack_segments: 0,
            ooo: Vec::new(),
            ooo_limit: 8,
            zero_window_deadline: None,
            zero_window_backoff_ms: 1000,
            zero_window_probes: 0,
            urgent_position: None,
            urgent_count: 0,
            rto: RtoEstimator::new(1000, 200, 60_000, 1),
            rto_deadline: None,
            rtt_sample: None,
            retransmit_count: 0,
            rto_timeouts: 0,
            last_retransmit_seq: None,
            rto_max_retransmits: 6,
            snd_wscale: 0,
            rcv_wscale: 0,
            offer_wscale: false,
            offer_timestamps: false,
            offer_sack: true,
            timestamps: false,
            ts_recent: 0,
            ts_recent_age: now,
            last_tsval: 0,
            last_ts_sent: None,
            peer_mss: 536,
            our_mss: 1460,
            mss_clamps: 0,
            mss_floor: 536,
            paws_drops: 0,
            sack_permitted: false,
            negotiated_wscale: None,
            ecn_enabled: false,
            ecn_negotiated: false,
            ecn_cwr_pending: false,
            ecn_ece_pending: false,
            retransmits_total: 0,
            dupack_count: 0,
            last_activity: now,
            last_adv_window: 0,
            last_ack_window: 65_535,
            cc: Box::new(crate::cc::NewReno::new(1460, 14_600, u32::MAX, 16_777_216)),
            rack: crate::recovery::Rack::new(10),
            rack_sent: Vec::new(),
            tlp: crate::recovery::Tlp::default(),
            tlp_enabled: true,
            tlp_probe_at: None,
            ecn: crate::recovery::Ecn::default(),
            recover_seq: 0,
            sacked: Vec::new(),
            high_rxt: 0,
            sack_recovery: false,
            rto_recovery: false,
            plp_ceiling: 0,
            plp: None,
            plp_decided: false,
            plp_probe: None,
            plp_next_ms: 0,
            plp_attempts: 0,
            undo: None,
            recovery_ts: 0,
            reorder_dist: 3,
            snd_max: None,
        }
    }

    pub(crate) fn new_passive(
        local: IpAddr,
        local_port: u16,
        remote: (IpAddr, u16),
        iss: u32,
        irs: u32,
        now: Instant,
    ) -> Self {
        Self {
            state: TcpState::SynReceived,
            local,
            local_port,
            remote: Some(remote),
            iss,
            irs,
            snd_una: iss,
            snd_nxt: iss.wrapping_add(1),
            snd_wnd: 0,
            max_snd_wnd: 65_535,
            rcv_nxt: irs.wrapping_add(1),
            rcv_wnd: 65_535,
            snd_wl1: irs,
            snd_wl2: iss,
            last_ack_sent: irs.wrapping_add(1),
            fin_sent: false,
            fin_acked: false,
            peer_fin: false,
            close_requested: false,
            passive: true,
            connect_deadline: None,
            connect_retries: 0,
            time_wait_deadline: None,
            fin_wait2_deadline: None,
            keepalive_deadline: None,
            keepalive_interval_ms: 7_200_000,
            keepalive_probe_interval_ms: 75_000,
            keepalive_probes: 9,
            keepalive_left: 9,
            challenge_tokens: 10,
            challenge_last: now,
            mss: 1460,
            nodelay: false,
            ack_deadline: None,
            ack_segments: 0,
            ooo: Vec::new(),
            ooo_limit: 8,
            zero_window_deadline: None,
            zero_window_backoff_ms: 1000,
            zero_window_probes: 0,
            urgent_position: None,
            urgent_count: 0,
            rto: RtoEstimator::new(1000, 200, 60_000, 1),
            rto_deadline: None,
            rtt_sample: None,
            retransmit_count: 0,
            rto_timeouts: 0,
            last_retransmit_seq: None,
            rto_max_retransmits: 6,
            snd_wscale: 0,
            rcv_wscale: 0,
            offer_wscale: false,
            offer_timestamps: false,
            offer_sack: true,
            timestamps: false,
            ts_recent: 0,
            ts_recent_age: now,
            last_tsval: 0,
            last_ts_sent: None,
            peer_mss: 536,
            our_mss: 1460,
            mss_clamps: 0,
            mss_floor: 536,
            paws_drops: 0,
            sack_permitted: false,
            negotiated_wscale: None,
            ecn_enabled: false,
            ecn_negotiated: false,
            ecn_cwr_pending: false,
            ecn_ece_pending: false,
            retransmits_total: 0,
            dupack_count: 0,
            last_activity: now,
            last_adv_window: 0,
            last_ack_window: 65_535,
            cc: Box::new(crate::cc::NewReno::new(1460, 14_600, u32::MAX, 16_777_216)),
            rack: crate::recovery::Rack::new(10),
            rack_sent: Vec::new(),
            tlp: crate::recovery::Tlp::default(),
            tlp_enabled: true,
            tlp_probe_at: None,
            ecn: crate::recovery::Ecn::default(),
            recover_seq: 0,
            sacked: Vec::new(),
            high_rxt: 0,
            sack_recovery: false,
            rto_recovery: false,
            plp_ceiling: 0,
            plp: None,
            plp_decided: false,
            plp_probe: None,
            plp_next_ms: 0,
            plp_attempts: 0,
            undo: None,
            recovery_ts: 0,
            reorder_dist: 3,
            snd_max: None,
        }
    }

    #[cfg(any(test, feature = "testutil"))]
    pub(crate) fn new_established_for_test(now: Instant) -> Self {
        let mut connection = Self::new_passive(
            IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            0,
            (IpAddr::V4(Ipv4Addr::UNSPECIFIED), 0),
            1,
            1,
            now,
        );
        connection.state = TcpState::Established;
        connection
    }
}

struct Segment<'a> {
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    ecn: u8,
    window: u16,
    urgent: u16,
    mss: Option<u16>,
    wscale: Option<u8>,
    tsval: Option<u32>,
    tsecr: Option<u32>,
    sack_permitted: bool,
    sack_blocks: Vec<(u32, u32)>,
    payload: &'a [u8],
}

impl Segment<'_> {
    fn syn(&self) -> bool {
        self.flags.syn()
    }
    fn fin(&self) -> bool {
        self.flags.fin()
    }
    fn rst(&self) -> bool {
        self.flags.rst()
    }
    fn ack_flag(&self) -> bool {
        self.flags.ack()
    }
    fn len(&self) -> u32 {
        u32::try_from(self.payload.len())
            .unwrap_or(u32::MAX)
            .wrapping_add(u32::from(self.syn()))
            .wrapping_add(u32::from(self.fin()))
    }
}

impl<D: Device> Stack<D> {
    /// The current TCP state of a socket.
    pub fn tcp_state(&self, socket: SocketId) -> Result<TcpState, Errno> {
        match &self.sockets.record(socket)?.kind {
            Kind::Tcp(connection) => Ok(connection.state),
            Kind::Listener { .. } => Ok(TcpState::Listen),
            _ => Err(Errno::ENOTSOCK),
        }
    }

    /// TIME-WAIT entries held and how many of them no longer have a socket record (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_time_wait_entries(&self) -> (usize, usize) {
        self.timewait.entries_detached()
    }

    /// Half-open connections of a listener (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_listener_half_open(&self, listener: SocketId) -> Vec<SocketId> {
        match self.sockets.record(listener).map(|record| &record.kind) {
            Ok(Kind::Listener { half_open, .. }) => half_open.sockets().map(SocketId).collect(),
            _ => Vec::new(),
        }
    }

    /// Last urgent pointer and urgent-segment count (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_urgent(&self, socket: SocketId) -> Option<(Option<u16>, usize)> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some((connection.urgent_position, connection.urgent_count)),
            _ => None,
        }
    }

    /// Out-of-order hole count (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_ooo_len(&self, socket: SocketId) -> Option<usize> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some(connection.ooo.len()),
            _ => None,
        }
    }

    /// Zero-window probes sent (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_zero_window_probes(&self, socket: SocketId) -> Option<usize> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some(connection.zero_window_probes),
            _ => None,
        }
    }

    /// TCP urgent segments seen (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_urgent_seen(&self) -> usize {
        self.tcp_urgent_seen
    }

    /// SYN,ACK attempts (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_synacks(&self) -> usize {
        self.tcp_synacks
    }

    /// TLP deadline (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_tlp_deadline(&self, socket: SocketId) -> Option<Instant> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => connection
                .tlp
                .deadline
                .map(|ms| Instant::from_millis(i64::from(ms))),
            _ => None,
        }
    }

    /// TCP segments received (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_debug(&self) -> &[(u16, u16, u8, u32, u32)] {
        &self.tcp_debug
    }

    /// The current send window and in-flight bytes (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_window(&self, socket: SocketId) -> Option<(u32, u32, u32)> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => {
                Some((connection.snd_wnd, connection.snd_una, connection.snd_nxt))
            }
            _ => None,
        }
    }

    /// Pretend the peer advertised `window`, modelling a stale or lost window update.
    #[cfg(any(test, feature = "testutil"))]
    pub fn test_set_tcp_snd_wnd(&mut self, socket: SocketId, window: u32) -> bool {
        if let Ok(record) = self.sockets.record_mut(socket)
            && let Kind::Tcp(connection) = &mut record.kind
        {
            connection.snd_wnd = window;
            true
        } else {
            false
        }
    }

    /// The local port and remote endpoint for a TCP socket (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_endpoints(&self, socket: SocketId) -> Option<(u16, Option<(IpAddr, u16)>)> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some((connection.local_port, connection.remote)),
            _ => None,
        }
    }

    /// Number of live syncache entries (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_syncache_len(&self) -> usize {
        self.syncache.len()
    }

    /// The syncache entry for a remote port: `(iss, irs, sack, timestamp)` (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_syncache_entry(&self, remote_port: u16) -> Option<(u32, u32, bool, bool)> {
        self.syncache
            .entries()
            .find(|entry| entry.key.remote.port == remote_port)
            .map(|entry| (entry.iss, entry.irs, entry.sack, entry.timestamp.is_some()))
    }

    /// The peer MSS held by a syncache entry (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_syncache_peer_mss(&self, remote_port: u16) -> Option<u16> {
        self.syncache
            .entries()
            .find(|entry| entry.key.remote.port == remote_port)
            .map(|entry| entry.peer_mss.unwrap_or(536))
    }

    /// The initial send sequence for a TCP socket (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_iss(&self, socket: SocketId) -> Option<u32> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some(connection.iss),
            _ => None,
        }
    }

    /// The receive-next sequence for a TCP socket (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_rcv_nxt(&self, socket: SocketId) -> Option<u32> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some(connection.rcv_nxt),
            _ => None,
        }
    }

    /// Negotiated TCP options (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    #[allow(clippy::type_complexity)]
    pub fn test_tcp_options_info(
        &self,
        socket: SocketId,
    ) -> Option<(u8, u8, bool, u32, u16, u16, usize, usize, u32)> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some((
                connection.snd_wscale,
                connection.rcv_wscale,
                connection.timestamps,
                connection.ts_recent,
                connection.peer_mss,
                connection.mss,
                connection.mss_clamps,
                connection.paws_drops,
                connection.last_tsval,
            )),
            _ => None,
        }
    }

    /// The negotiated window scale, if the peer offered WS (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_negotiated_wscale(&self, socket: SocketId) -> Option<Option<u8>> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some(connection.negotiated_wscale),
            _ => None,
        }
    }

    /// Whether SACK-permitted was negotiated (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_sack_permitted(&self, socket: SocketId) -> Option<bool> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some(connection.sack_permitted),
            _ => None,
        }
    }

    /// Age anchor of the last accepted timestamp (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_ts_age(&self, socket: SocketId) -> Option<Instant> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some(connection.ts_recent_age),
            _ => None,
        }
    }

    /// RTO estimator state (host tests): srtt, rttvar, rto, retransmits, has_sample.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_rto(&self, socket: SocketId) -> Option<(u64, u64, u64, u8, bool)> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some((
                connection.rto.srtt_ms(),
                connection.rto.rttvar_ms(),
                connection.rto.rto_ms(),
                connection.retransmit_count,
                connection.rto.has_sample(),
            )),
            _ => None,
        }
    }

    /// RTO deadline (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_rto_deadline(&self, socket: SocketId) -> Option<Instant> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => connection.rto_deadline,
            _ => None,
        }
    }

    /// Last retransmitted sequence (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_last_retransmit(&self, socket: SocketId) -> Option<u32> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => connection.last_retransmit_seq,
            _ => None,
        }
    }

    /// Send-queue front and sequence (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_tx_head(&self, socket: SocketId) -> Option<(u32, Vec<u8>)> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => {
                let record = self.sockets.record(socket).ok()?;
                Some((
                    connection.snd_una,
                    record.tx.iter().take(8).copied().collect(),
                ))
            }
            _ => None,
        }
    }

    /// Total delivered TCP data bytes (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_data_delivered(&self) -> usize {
        self.tcp_data_delivered
    }

    /// Total RTO timeouts observed on a socket.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_rto_timeouts(&self, socket: SocketId) -> Option<usize> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => Some(connection.rto_timeouts),
            _ => None,
        }
    }

    /// Configure an MD5 key for a TCP connection.
    #[cfg(feature = "tcp-md5")]
    pub fn set_md5_key(&mut self, socket: SocketId, key: Option<&[u8]>) -> Result<(), Errno> {
        let record = self.sockets.record(socket)?;
        let Kind::Tcp(connection) = &record.kind else {
            return Err(Errno::ENOTSOCK);
        };
        let port = connection.local_port;
        match key {
            Some(key) => {
                self.md5_keys.insert(port, key.to_vec());
            }
            None => {
                self.md5_keys.remove(&port);
            }
        }
        Ok(())
    }

    /// Configure an MD5 key for a TCP connection.
    #[cfg(not(feature = "tcp-md5"))]
    pub fn set_md5_key(&mut self, _socket: SocketId, _key: Option<&[u8]>) -> Result<(), Errno> {
        Err(Errno::EOPNOTSUPP)
    }

    /// Configure TCP-AO keys for a TCP connection.
    #[cfg(feature = "tcp-ao")]
    pub fn set_ao_keys(
        &mut self,
        socket: SocketId,
        keys: &[crate::signature::AoKey],
    ) -> Result<(), Errno> {
        let record = self.sockets.record(socket)?;
        let Kind::Tcp(connection) = &record.kind else {
            return Err(Errno::ENOTSOCK);
        };
        let port = connection.local_port;
        if keys.is_empty() {
            self.ao_keys.remove(&port);
        } else {
            self.ao_keys.insert(port, keys.to_vec());
        }
        Ok(())
    }

    /// Configure TCP-AO keys for a TCP connection.
    #[cfg(not(feature = "tcp-ao"))]
    pub fn set_ao_keys(
        &mut self,
        _socket: SocketId,
        _keys: &[crate::signature::AoKey],
    ) -> Result<(), Errno> {
        Err(Errno::EOPNOTSUPP)
    }

    /// Signature verification failures (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_signature_failures(&self) -> usize {
        self.signature_failures
    }

    /// The most recent urgent pointer delivered out-of-band on this socket.
    pub fn tcp_urgent(&self, socket: SocketId) -> Result<Option<u16>, Errno> {
        match &self.sockets.record(socket)?.kind {
            Kind::Tcp(connection) => Ok(connection.urgent_position),
            _ => Err(Errno::ENOTSOCK),
        }
    }

    /// The last asynchronous TCP error recorded on a socket.
    pub fn tcp_error(&self, socket: SocketId) -> Result<Option<Errno>, Errno> {
        Ok(self.sockets.record(socket)?.error)
    }

    /// Abort a TCP connection with a RST where the RFC requires one.
    pub fn abort(&mut self, socket: SocketId) -> Result<(), Errno> {
        let (synchronized, seq, local, remote, remote_port, port, window) = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            let synchronized = matches!(
                connection.state,
                TcpState::SynReceived
                    | TcpState::Established
                    | TcpState::FinWait1
                    | TcpState::FinWait2
                    | TcpState::CloseWait
                    | TcpState::Closing
                    | TcpState::LastAck
            );
            let remote = connection.remote.ok_or(Errno::ENOTCONN)?;
            (
                synchronized,
                connection.snd_nxt,
                connection.local,
                remote.0,
                remote.1,
                connection.local_port,
                connection.rcv_wnd,
            )
        };
        if synchronized {
            let _ = self.send_tcp_packet(
                local,
                remote,
                port,
                remote_port,
                TcpFlags::RST,
                seq,
                0,
                window,
                &[],
                false,
            );
        }
        let record = self.sockets.record_mut(socket)?;
        if let Kind::Tcp(connection) = &mut record.kind {
            connection.state = TcpState::Closed;
        }
        record.closed = true;
        record.rx.clear();
        record.tx.clear();
        Ok(())
    }

    /// Enable keepalive on a connection.
    pub fn tcp_set_keepalive(
        &mut self,
        socket: SocketId,
        interval: crate::Duration,
        probes: u8,
    ) -> Result<(), Errno> {
        if self.tunables.keepalive_enable == 0 {
            return Err(Errno::EOPNOTSUPP);
        }
        let interval_ms = if interval.as_millis() == 0 {
            self.tunables.keepalive_interval
        } else {
            interval.as_millis()
        };
        let probes = if probes == 0 {
            u8::try_from(self.tunables.keepalive_probes).unwrap_or(9)
        } else {
            probes
        };
        let interval = crate::Duration::from_millis(interval_ms);
        let record = self.sockets.record_mut(socket)?;
        let Kind::Tcp(connection) = &mut record.kind else {
            return Err(Errno::ENOTSOCK);
        };
        connection.keepalive_interval_ms = interval.as_millis();
        connection.keepalive_probe_interval_ms = interval.as_millis().min(75_000);
        connection.keepalive_probes = probes;
        connection.keepalive_left = probes;
        connection.keepalive_deadline =
            Some(Instant::from_millis(self.now.as_millis().saturating_add(
                i64::try_from(interval.as_millis()).unwrap_or(i64::MAX),
            )));
        Ok(())
    }

    pub(crate) fn tcp_connect_internal<A: Into<IpAddr>>(
        &mut self,
        remote: (A, u16),
        local_port: Option<u16>,
    ) -> Result<SocketId, Errno> {
        let remote = (remote.0.into(), remote.1);
        if remote.1 == 0 {
            return Err(Errno::EADDRNOTAVAIL);
        }
        if self.live_tcp_connections()
            >= usize::try_from(self.tunables.connection_max).unwrap_or(usize::MAX)
        {
            self.counters.attempt_fails = self.counters.attempt_fails.wrapping_add(1);
            return Err(Errno::EMFILE);
        }
        let local_port = match local_port {
            Some(0) => return Err(Errno::EADDRNOTAVAIL),
            Some(port) if self.port_in_use(port) => return Err(Errno::EADDRINUSE),
            Some(port) => port,
            None => self.alloc_ephemeral_port()?,
        };
        let local = match remote.0 {
            IpAddr::V4(_) => IpAddr::V4(
                self.ipv4
                    .map_or(Ipv4Addr::UNSPECIFIED, |config| config.addr),
            ),
            IpAddr::V6(_) => self
                .usable_ipv6()
                .map_or(IpAddr::V6(Ipv6Addr::UNSPECIFIED), IpAddr::V6),
        };
        let iss = self.next_isn(local, local_port, remote.0, remote.1);
        let mut connection = TcpConnection::new_active(local, local_port, remote, iss, self.now);
        self.configure_tcp_connection(&mut connection, remote.0);
        let socket = self.sockets.alloc(
            Kind::Tcp(connection),
            self.recv_buffer_cap(),
            self.send_buffer_cap(),
        )?;
        self.counters.active_opens = self.counters.active_opens.wrapping_add(1);
        let _ = self.send_syn(socket);
        Ok(socket)
    }

    pub(crate) fn live_tcp_connections(&self) -> usize {
        self.sockets
            .records
            .values()
            .filter(|record| !record.closed && matches!(record.kind, Kind::Tcp(_)))
            .count()
    }

    fn configure_tcp_connection(&self, connection: &mut TcpConnection, remote: IpAddr) {
        connection.rto = RtoEstimator::new(
            self.tunables.rto_initial,
            self.tunables.rto_min,
            self.tunables.rto_max,
            self.config.clock_granularity_ms,
        );
        connection.rto_max_retransmits =
            u8::try_from(self.tunables.rto_max_retries).unwrap_or(u8::MAX);
        connection.offer_wscale = self.tunables.window_scale_enable != 0;
        connection.offer_timestamps = self.tunables.timestamps_enable != 0;
        connection.offer_sack = self.tunables.sack_enable != 0;
        connection.ecn_enabled = self.tunables.ecn_enable != 0;
        connection.ecn.negotiated = false;
        connection.rack = crate::recovery::Rack::new(
            u32::try_from(self.tunables.reordering_window_base).unwrap_or(10),
        );
        connection.tlp_enabled = self.tunables.tlp_enable != 0;
        connection.nodelay = self.tunables.nagle == 0;
        connection.ooo_limit = usize::try_from(self.tunables.reassembly_holes_max).unwrap_or(32);
        connection.keepalive_interval_ms = self.tunables.keepalive_interval;
        connection.keepalive_probes =
            u8::try_from(self.tunables.keepalive_probes).unwrap_or(u8::MAX);
        connection.keepalive_left = connection.keepalive_probes;
        connection.challenge_tokens =
            u32::try_from(self.tunables.challenge_ack_burst).unwrap_or(u32::MAX);
        connection.rcv_wscale = choose_wscale(self.recv_buffer_cap());
        let mss_default = u16::try_from(self.tunables.mss_default).unwrap_or(1460);
        let mut mss = our_mss(remote, self.device.capabilities(), mss_default);
        let interface_mss = mss;
        if self.tunables.ignore_path_mtu == 0
            && let IpAddr::V4(address) = remote
            && let Some((mtu, inserted)) = self.pmtu4.get(&address)
            && self.now.saturating_since(*inserted).as_millis() < self.tunables.pmtu_cache_ttl
        {
            let path = u16::try_from(mtu.saturating_sub(40)).unwrap_or(u16::MAX);
            mss = mss.min(path);
        }
        connection.mss_floor = u16::try_from(self.tunables.mss_floor).unwrap_or(536);
        let ceiling = u16::try_from(self.tunables.mss_ceiling).unwrap_or(8960);
        connection.our_mss = mss
            .max(connection.mss_floor)
            .min(ceiling)
            .max(connection.mss_floor);
        connection.mss = connection.our_mss;
        connection.plp_ceiling = interface_mss
            .max(connection.mss_floor)
            .min(ceiling)
            .max(connection.mss_floor);
        connection.cc = self.new_cc(connection.our_mss);
    }

    fn new_cc(&self, mss: u16) -> Box<dyn crate::cc::CongestionControl> {
        let mss = u32::from(mss.max(1));
        let max = u32::try_from(self.tunables.cwnd_max)
            .unwrap_or(16_777_216)
            .max(2 * mss);
        let floor = u32::try_from(self.tunables.cwnd_min)
            .unwrap_or(2)
            .saturating_mul(mss);
        let initial = u32::try_from(self.tunables.initial_window)
            .unwrap_or(10)
            .saturating_mul(mss)
            .max(floor)
            .max(2 * mss);
        if self.tunables.cc_algorithm == 0 {
            let mut cc = crate::cc::NewReno::new(mss, initial, max, max);
            cc.set_dup_threshold(u32::try_from(self.tunables.dup_ack_threshold).unwrap_or(3));
            Box::new(cc)
        } else {
            Box::new(crate::cc::Cubic::new(mss, initial, max, max))
        }
    }

    pub(crate) fn send_buffer_cap(&self) -> usize {
        let default = usize::try_from(self.tunables.send_buffer_default).unwrap_or(65_536);
        let max = usize::try_from(self.tunables.send_buffer_max).unwrap_or(4_194_304);
        default.min(max).min(self.config.tx_queue_bytes)
    }

    pub(crate) fn recv_buffer_cap(&self) -> usize {
        let default = usize::try_from(self.tunables.recv_buffer_default).unwrap_or(65_536);
        let max = usize::try_from(self.tunables.recv_buffer_max).unwrap_or(4_194_304);
        default.min(max).min(self.config.rx_queue_bytes)
    }

    fn next_isn(
        &mut self,
        local: IpAddr,
        local_port: u16,
        remote: IpAddr,
        remote_port: u16,
    ) -> u32 {
        let mut increment = [0u8; 4];
        self.rng.fill(&mut increment);
        let cookie = self.cookie_secret.to_le_bytes();
        let mut secret = [0u8; ISN_SECRET_LENGTH];
        secret[..8].copy_from_slice(&cookie);
        secret[8..].copy_from_slice(&cookie);
        let increment = if self.tunables.isn_strength == 0 {
            0
        } else {
            u32::from_le_bytes(increment)
        };
        let entropy = IsnEntropy::new(secret, increment);
        let conn = IsnConn::new(local, local_port, remote, remote_port);
        let ticks = u64::try_from(self.now.as_millis()).unwrap_or(0);
        self.isn.next_isn(&conn, ticks, entropy)
    }

    pub(crate) fn tcp_input(
        &mut self,
        source: IpAddr,
        destination: IpAddr,
        ecn: u8,
        payload: &[u8],
        now: Instant,
    ) {
        let context = match (source, destination) {
            (IpAddr::V4(source), IpAddr::V4(destination)) => {
                ChecksumContext::ipv4(to_wire4(source), to_wire4(destination))
            }
            (IpAddr::V6(source), IpAddr::V6(destination)) => {
                ChecksumContext::ipv6(to_wire6(source), to_wire6(destination))
            }
            _ => return,
        };
        let packet = match TcpPacket::parse(payload, context) {
            Ok(packet) => packet,
            Err(error) => {
                if error.tag() == "tcp.checksum" {
                    self.counters.in_checksum_errors =
                        self.counters.in_checksum_errors.wrapping_add(1);
                } else {
                    self.counters.in_header_errors = self.counters.in_header_errors.wrapping_add(1);
                    if error.tag().starts_with("tcp.option") {
                        self.counters.option_malformed =
                            self.counters.option_malformed.wrapping_add(1);
                    }
                }
                self.drops = self.drops.saturating_add(1);
                return;
            }
        };
        if packet.flags().rst() {
            self.counters.rst_rcvd = self.counters.rst_rcvd.wrapping_add(1);
        }
        #[cfg(any(test, feature = "testutil"))]
        self.tcp_debug.push((
            packet.src_port(),
            packet.dst_port(),
            packet.flags().bits(),
            packet.sequence(),
            packet.acknowledgement(),
        ));
        let mut mss = None;
        let mut wscale = None;
        let mut tsval = None;
        let mut tsecr = None;
        let mut sack_permitted = false;
        let mut sack_blocks: Vec<(u32, u32)> = Vec::new();
        for option in packet.options() {
            match option {
                Ok(TcpOption::MaxSegmentSize(value)) => mss = Some(value),
                Ok(TcpOption::WindowScale(value)) => wscale = Some(value),
                Ok(TcpOption::SackPermitted) => sack_permitted = true,
                Ok(TcpOption::Sack(blocks)) => sack_blocks.extend(blocks.take(4)),
                Ok(TcpOption::Timestamps { tsval: v, tsecr: e }) => {
                    tsval = Some(v);
                    tsecr = Some(e);
                }
                Ok(TcpOption::Unknown { .. }) => {
                    self.counters.option_unknown = self.counters.option_unknown.wrapping_add(1);
                }
                Ok(TcpOption::EndOfList | TcpOption::NoOperation) => {}
                Ok(_) => {}
                Err(_) => {
                    self.counters.option_malformed = self.counters.option_malformed.wrapping_add(1);
                }
            }
        }
        let segment = Segment {
            seq: packet.sequence(),
            ack: packet.acknowledgement(),
            flags: packet.flags(),
            ecn: ecn & crate::ecn::ECN_MASK,
            window: packet.window(),
            urgent: packet.urgent_pointer(),
            mss,
            wscale,
            tsval,
            tsecr,
            sack_permitted,
            sack_blocks,
            payload: packet.payload(),
        };
        let source_port = packet.src_port();
        let destination_port = packet.dst_port();
        #[cfg(feature = "tcp-md5")]
        if let Some(key) = self.md5_keys.get(&destination_port).cloned() {
            let mut digest = None;
            for option in packet.options().flatten() {
                if let TcpOption::Md5 { digest: value } = option {
                    digest = Some(value);
                }
            }
            let Some(received) = digest else {
                self.signature_failures = self.signature_failures.saturating_add(1);
                self.drops = self.drops.saturating_add(1);
                return;
            };
            let mut header = [0u8; 20];
            header.copy_from_slice(&payload[..20]);
            header[16] = 0;
            header[17] = 0;
            let mut input = signature_pseudo(
                IpAddr::V4(match (source, destination) {
                    (IpAddr::V4(_), IpAddr::V4(value)) => value,
                    _ => Ipv4Addr::UNSPECIFIED,
                }),
                IpAddr::V4(match (source, destination) {
                    (IpAddr::V4(value), IpAddr::V4(_)) => value,
                    _ => Ipv4Addr::UNSPECIFIED,
                }),
                payload.len(),
            );
            input.clear();
            input = signature_pseudo(source, destination, payload.len());
            input.extend_from_slice(&header);
            input.extend_from_slice(packet.payload());
            input.extend_from_slice(&key);
            let expected = crate::signature::md5(&input);
            if received != expected {
                self.signature_failures = self.signature_failures.saturating_add(1);
                self.drops = self.drops.saturating_add(1);
                return;
            }
        }
        if segment.flags.urg() && self.tunables.urgent_mode != 0 {
            #[cfg(any(test, feature = "testutil"))]
            {
                self.tcp_urgent_seen = self.tcp_urgent_seen.saturating_add(1);
            }
            let exact =
                self.sockets
                    .records
                    .iter_mut()
                    .find_map(|(_, record)| match &mut record.kind {
                        Kind::Tcp(connection)
                            if connection.local == destination
                                && connection.local_port == destination_port
                                && connection.remote == Some((source, source_port)) =>
                        {
                            Some(connection)
                        }
                        _ => None,
                    });
            if let Some(connection) = exact {
                connection.urgent_position = Some(segment.urgent);
                connection.urgent_count = connection.urgent_count.saturating_add(1);
            }
        }

        self.absorb_detached();
        let exact =
            self.sockets
                .find_connection(destination, destination_port, source, source_port);
        if let Some(socket) = exact {
            self.counters.in_segments = self.counters.in_segments.wrapping_add(1);
            self.tcp_input_established(socket, &segment, now);
            return;
        }
        let time_wait_key = crate::table::ConnectionKey {
            local: destination,
            local_port: destination_port,
            remote: source,
            remote_port: source_port,
        };
        if let Some(info) = self.timewait.get(&time_wait_key).and_then(|slot| slot.info) {
            self.counters.in_segments = self.counters.in_segments.wrapping_add(1);
            if self.detached_time_wait_input(&time_wait_key, info, &segment, now) {
                return;
            }
        }
        let key = syncache_key(destination, destination_port, source, source_port);
        if segment.rst() && self.syncache.abort(&key, segment.seq) == AbortOutcome::Removed {
            self.counters.in_segments = self.counters.in_segments.wrapping_add(1);
            return;
        }
        if segment.ack_flag() && !segment.syn() {
            let ack = AckRequest {
                key,
                seq: segment.seq,
                ack: segment.ack,
                window: segment.window,
                timestamp: segment.tsval,
            };
            if self.promote_syncache(destination, destination_port, ack, &segment, now) {
                self.counters.in_segments = self.counters.in_segments.wrapping_add(1);
                return;
            }
        }
        let listener = self.sockets.records.iter().find_map(|(raw_id, record)| {
            if record.closed {
                return None;
            }
            match record.kind {
                Kind::Listener { port, .. } if port == destination_port => Some(SocketId(*raw_id)),
                _ => None,
            }
        });
        if let Some(listener) = listener {
            self.counters.in_segments = self.counters.in_segments.wrapping_add(1);
            self.tcp_listen_input(
                listener,
                source,
                destination,
                source_port,
                destination_port,
                &segment,
                now,
            );
            return;
        }
        if segment.rst() {
            return;
        }
        if is_group(destination) {
            return;
        }
        let (flags, seq, ack) = if segment.ack_flag() {
            (TcpFlags::RST, segment.ack, 0)
        } else {
            (
                flags_or(TcpFlags::RST, TcpFlags::ACK),
                0,
                segment.seq.wrapping_add(segment.len()),
            )
        };
        self.counters.rsts_for_unsent = self.counters.rsts_for_unsent.wrapping_add(1);
        let _ = self.send_tcp_raw(
            destination,
            source,
            destination_port,
            source_port,
            flags,
            seq,
            ack,
        );
    }

    /// Forget half-open entries whose connection closed or left SYN-RECEIVED.
    #[allow(clippy::too_many_arguments)]
    fn tcp_listen_input(
        &mut self,
        listener: SocketId,
        source: IpAddr,
        destination: IpAddr,
        source_port: u16,
        destination_port: u16,
        segment: &Segment<'_>,
        now: Instant,
    ) {
        if segment.rst() {
            return;
        }
        if segment.ack_flag() {
            if self.tunables.syn_cookies_enable != 0
                && self.syn_cookie_valid(
                    destination,
                    source,
                    destination_port,
                    source_port,
                    segment.ack,
                )
            {
                self.finish_syn_cookie(
                    listener,
                    source,
                    destination,
                    source_port,
                    destination_port,
                    segment,
                );
                return;
            }
            if self.tunables.syn_cookies_enable != 0 {
                self.counters.syn_cookie_invalid = self.counters.syn_cookie_invalid.wrapping_add(1);
            }
            let _ = self.send_tcp_raw(
                destination,
                source,
                destination_port,
                source_port,
                TcpFlags::RST,
                segment.ack,
                0,
            );
            return;
        }
        if !segment.syn() {
            return;
        }
        if self.live_tcp_connections()
            >= usize::try_from(self.tunables.connection_max).unwrap_or(usize::MAX)
        {
            self.counters.listen_drops = self.counters.listen_drops.wrapping_add(1);
            self.drops = self.drops.saturating_add(1);
            return;
        }
        let backlog = match self.sockets.record(listener).map(|record| &record.kind) {
            Ok(Kind::Listener { _backlog, .. }) => *_backlog,
            _ => return,
        };
        let half_open_limit = usize::try_from(self.tunables.half_open_limit).unwrap_or(usize::MAX);
        let limit = backlog.min(half_open_limit);
        let queued = self
            .syncache
            .entries()
            .filter(|entry| entry.key.local.port == destination_port)
            .count();
        if queued >= limit {
            if self.tunables.syn_cookies_enable != 0 {
                let iss = self.syn_cookie(
                    destination,
                    source,
                    destination_port,
                    source_port,
                    self.cookie_slot(self.now.as_millis()),
                );
                self.option_override = Some(vec![2, 4, 0x05, 0xb4, 1, 1, 3, 3, 0, 0]);
                let _ = self.send_tcp_packet(
                    destination,
                    source,
                    destination_port,
                    source_port,
                    flags_or(TcpFlags::SYN, TcpFlags::ACK),
                    iss,
                    segment.seq.wrapping_add(1),
                    65_535,
                    &[],
                    true,
                );
                self.counters.syn_cookie_sent = self.counters.syn_cookie_sent.wrapping_add(1);
                return;
            }
            self.counters.listen_drops = self.counters.listen_drops.wrapping_add(1);
            self.counters.half_open_drops = self.counters.half_open_drops.wrapping_add(1);
            self.drops = self.drops.saturating_add(1);
            return;
        }
        let key = syncache_key(destination, destination_port, source, source_port);
        if let Some(existing) = self.syncache.lookup(&key).cloned()
            && seq_lt(segment.seq, existing.irs)
        {
            let _ = self.send_tcp_packet(
                destination,
                source,
                destination_port,
                source_port,
                flags_or(TcpFlags::RST, TcpFlags::ACK),
                0,
                segment.seq.wrapping_add(segment.len()),
                0,
                &[],
                false,
            );
            return;
        }
        let iss = self.next_isn(destination, destination_port, source, source_port);
        let request = SynRequest {
            key,
            irs: segment.seq,
            iss,
            receive_window: u32::try_from(self.recv_buffer_cap()).unwrap_or(u32::MAX),
            options: SynOptions {
                mss: segment.mss,
                window_scale: segment.wscale,
                sack_permitted: segment.sack_permitted,
                timestamp: segment.tsval,
                ecn: self.tunables.ecn_enable != 0 && (segment.flags.ece() || segment.flags.cwr()),
                no_options: false,
            },
            ip_ttl: 64,
            ip_tos: 0,
        };
        let now_ms = u64::try_from(now.as_millis()).unwrap_or(0);
        match self.syncache.insert(request, now_ms) {
            InsertOutcome::Inserted { .. } | InsertOutcome::Duplicate { .. } => {
                self.counters.passive_opens = self.counters.passive_opens.wrapping_add(1);
                let _ = self.send_syncache_syn_ack(&key);
            }
            InsertOutcome::Rejected => {
                if self.tunables.syn_cookies_enable != 0 {
                    let iss = self.syn_cookie(
                        destination,
                        source,
                        destination_port,
                        source_port,
                        self.cookie_slot(self.now.as_millis()),
                    );
                    self.option_override = Some(vec![2, 4, 0x05, 0xb4, 1, 1, 3, 3, 0, 0]);
                    let _ = self.send_tcp_packet(
                        destination,
                        source,
                        destination_port,
                        source_port,
                        flags_or(TcpFlags::SYN, TcpFlags::ACK),
                        iss,
                        segment.seq.wrapping_add(1),
                        65_535,
                        &[],
                        true,
                    );
                    self.counters.syn_cookie_sent = self.counters.syn_cookie_sent.wrapping_add(1);
                    return;
                }
                self.counters.listen_drops = self.counters.listen_drops.wrapping_add(1);
                self.drops = self.drops.saturating_add(1);
            }
        }
    }

    fn send_syncache_syn_ack(&mut self, key: &SyncKey) -> Result<(), Errno> {
        let Some(entry) = self.syncache.lookup(key).cloned() else {
            return Err(Errno::ENOENT);
        };
        let remote = syncache_ip(entry.key.remote.address);
        let default_mss = u16::try_from(self.tunables.mss_default).unwrap_or(1_460);
        let mss = our_mss(remote, self.device.capabilities(), default_mss);
        let mut options = vec![2, 4];
        options.extend_from_slice(&mss.to_be_bytes());
        if entry.window_scale {
            options.extend_from_slice(&[3, 3, entry.requested_r_scale]);
        }
        if entry.sack {
            options.extend_from_slice(&[4, 2]);
        }
        if let Some(reflect) = entry.timestamp {
            let tsval = u32::try_from(self.now.as_millis()).unwrap_or(0);
            options.extend_from_slice(&[8, 10]);
            options.extend_from_slice(&tsval.to_be_bytes());
            options.extend_from_slice(&reflect.to_be_bytes());
        }
        self.option_override = Some(options);
        let mut flags = flags_or(TcpFlags::SYN, TcpFlags::ACK);
        if entry.ecn {
            flags = flags_or(flags, TcpFlags::ECE);
        }
        self.send_tcp_packet(
            syncache_ip(entry.key.local.address),
            remote,
            entry.key.local.port,
            entry.key.remote.port,
            flags,
            entry.iss,
            entry.irs.wrapping_add(1),
            entry.receive_window,
            &[],
            true,
        )
    }

    fn promote_syncache(
        &mut self,
        local: IpAddr,
        local_port: u16,
        ack: AckRequest,
        segment: &Segment<'_>,
        now: Instant,
    ) -> bool {
        let outcome = self.syncache.promote(ack);
        let entry = match outcome {
            PromoteOutcome::Promoted { entry, .. } => entry,
            PromoteOutcome::Missing => return false,
            PromoteOutcome::BadAck => {
                let _ = self.send_tcp_raw(
                    local,
                    syncache_ip(ack.key.remote.address),
                    local_port,
                    ack.key.remote.port,
                    TcpFlags::RST,
                    segment.ack,
                    0,
                );
                return true;
            }
            PromoteOutcome::BadSequence => {
                if let Some(entry) = self.syncache.lookup(&ack.key).cloned() {
                    let _ = self.send_tcp_packet(
                        local,
                        syncache_ip(entry.key.remote.address),
                        local_port,
                        entry.key.remote.port,
                        TcpFlags::ACK,
                        entry.iss,
                        entry.irs.wrapping_add(1),
                        entry.receive_window,
                        &[],
                        false,
                    );
                }
                return true;
            }
            PromoteOutcome::MissingTimestamp | PromoteOutcome::StaleTimestamp => return true,
        };

        let remote = syncache_ip(entry.key.remote.address);
        let mut connection = TcpConnection::new_passive(
            local,
            local_port,
            (remote, entry.key.remote.port),
            entry.iss,
            entry.irs,
            now,
        );
        self.configure_tcp_connection(&mut connection, remote);
        connection.state = TcpState::Established;
        connection.passive = true;
        connection.connect_deadline = None;
        connection.snd_una = segment.ack;
        connection.snd_nxt = segment.ack;
        connection.snd_wnd = u32::from(segment.window) << entry.requested_s_scale.min(14);
        connection.max_snd_wnd = segment.window;
        connection.snd_wl1 = entry.irs;
        connection.snd_wl2 = entry.iss;
        connection.rcv_nxt = entry.irs.wrapping_add(1);
        connection.timestamps = entry.timestamp.is_some() && connection.offer_timestamps;
        connection.ts_recent = entry.timestamp.unwrap_or(0);
        connection.ts_recent_age = now;
        connection.sack_permitted = entry.sack && connection.offer_sack;
        connection.negotiated_wscale = entry.window_scale.then_some(entry.requested_s_scale);
        connection.snd_wscale = entry.requested_s_scale.min(14);
        connection.rcv_wscale = entry.requested_r_scale;
        if let Some(peer_mss) = entry.peer_mss {
            let clamped = peer_mss.clamp(connection.mss_floor, connection.our_mss);
            if clamped != peer_mss {
                connection.mss_clamps = connection.mss_clamps.saturating_add(1);
            }
            connection.peer_mss = clamped;
        }
        let option_reduction = if connection.timestamps { 12 } else { 0 };
        connection.mss = connection
            .peer_mss
            .min(connection.our_mss)
            .saturating_sub(option_reduction)
            .max(1);
        connection.ecn_negotiated = entry.ecn && connection.ecn_enabled;
        connection.last_activity = now;

        let Ok(socket) = self.sockets.alloc(
            Kind::Tcp(connection),
            self.recv_buffer_cap(),
            self.send_buffer_cap(),
        ) else {
            return true;
        };
        let listener = self.sockets.records.iter().find_map(|(raw_id, record)| {
            if record.closed {
                return None;
            }
            match record.kind {
                Kind::Listener { port, .. } if port == local_port => Some(SocketId(*raw_id)),
                _ => None,
            }
        });
        if let Some(listener) = listener
            && let Ok(record) = self.sockets.record_mut(listener)
            && let Kind::Listener { pending, .. } = &mut record.kind
        {
            let limit = usize::try_from(self.tunables.accept_queue_limit).unwrap_or(128);
            if pending.len() < limit {
                let _ = pending.push(crate::HalfOpen {
                    remote,
                    remote_port: entry.key.remote.port,
                    irs: entry.irs,
                    iss: entry.iss,
                    socket: socket.0,
                });
            } else {
                self.counters.accept_queue_drops = self.counters.accept_queue_drops.wrapping_add(1);
            }
        }
        if !segment.payload.is_empty() {
            self.tcp_input_established(socket, segment, now);
        }
        true
    }

    #[allow(unused_assignments)]
    fn tcp_input_established(&mut self, socket: SocketId, segment: &Segment<'_>, now: Instant) {
        let mut transition = None;
        let mut send_ack = false;
        let mut send_challenge = false;
        let mut send_rst = false;
        let mut drop_with_reset = false;
        let mut send_syn_ack = false;
        let mut rst_seq = 0;
        let mut close_error = None;
        let mut time_wait = false;
        let mut time_wait_refreshed = false;
        let mut fin_wait2 = false;
        let mut closed = false;
        let mut pending_listener = None;
        let mut ack_advanced = None;
        let mut eifel_distance: Option<u32> = None;
        let mut incoming: Option<(u32, &[u8])> = None;
        let mut fin_after_data = false;
        let mut fast_retransmit = false;
        {
            let Ok(record) = self.sockets.record_mut(socket) else {
                return;
            };
            let Kind::Tcp(connection) = &mut record.kind else {
                return;
            };
            // FreeBSD `tcp_ecn_input_segment`: a CE-marked packet makes the receiver echo
            // ECE and schedule CWR; a received CWR clears the ECE echo.
            if connection.ecn_enabled && connection.ecn_negotiated {
                if segment.ecn == crate::ecn::ECN_CE {
                    let react = connection.ecn.on_ce();
                    connection.ecn_ece_pending = true;
                    connection.ecn_cwr_pending = true;
                    self.counters.ecn_ce_marks = self.counters.ecn_ce_marks.wrapping_add(1);
                    if react {
                        let flight = connection.snd_nxt.wrapping_sub(connection.snd_una);
                        let mut context = crate::CcContext {
                            mss: u32::from(connection.mss),
                            flight,
                            pipe: flight,
                            rtt: None,
                            snd_nxt: connection.snd_nxt,
                            snd_una: connection.snd_una,
                        };
                        connection
                            .cc
                            .on_loss(&mut context, crate::cc::LossKind::Ecn);
                        connection.recover_seq = connection.snd_nxt;
                        self.counters.ecn_ce_react = self.counters.ecn_ce_react.wrapping_add(1);
                    }
                }
                if segment.flags.cwr() {
                    connection.ecn_ece_pending = false;
                }
            }
            'transition: {
                // FreeBSD `tcp_do_segment` validates RST before the state dispatch
                // (RFC 5961 section 3.2), then drops a SYN in a synchronized state
                // (section 4.2).  SYN_SENT and TIME_WAIT keep their own split bodies.
                if segment.rst()
                    && !matches!(connection.state, TcpState::SynSent | TcpState::TimeWait)
                {
                    match reset_disposition(connection, segment.seq) {
                        ResetDisposition::Accept => {
                            if connection.state == TcpState::SynReceived {
                                close_error = Some(Errno::ECONNREFUSED);
                                self.counters.attempt_fails =
                                    self.counters.attempt_fails.wrapping_add(1);
                            } else {
                                close_error = Some(Errno::ECONNRESET);
                            }
                            if connection.state == TcpState::Established {
                                self.counters.estab_resets =
                                    self.counters.estab_resets.wrapping_add(1);
                            }
                            closed = true;
                        }
                        ResetDisposition::Challenge => {
                            send_challenge = true;
                            self.counters.rst_challenge =
                                self.counters.rst_challenge.wrapping_add(1);
                        }
                        ResetDisposition::Ignore => {
                            self.counters.rst_out_of_window =
                                self.counters.rst_out_of_window.wrapping_add(1);
                        }
                    }
                    break 'transition;
                }
                if segment.syn()
                    && !matches!(
                        connection.state,
                        TcpState::SynSent | TcpState::SynReceived | TcpState::TimeWait
                    )
                {
                    send_challenge = true;
                    break 'transition;
                }
                // FreeBSD validates SEG.ACK against SND.MAX before the synchronized
                // state body.  ESTABLISHED/FIN-WAIT-1 already do this in their body.
                if segment.ack_flag()
                    && matches!(
                        connection.state,
                        TcpState::FinWait2
                            | TcpState::CloseWait
                            | TcpState::Closing
                            | TcpState::LastAck
                    )
                    && !ack_window_ok(connection, segment.ack)
                {
                    if seq_gt(segment.ack, connection.highest_sent()) {
                        self.counters.acks_for_unsent =
                            self.counters.acks_for_unsent.wrapping_add(1);
                    } else {
                        self.counters.ack_challenge = self.counters.ack_challenge.wrapping_add(1);
                    }
                    send_challenge = true;
                    break 'transition;
                }
                match connection.state {
                    TcpState::SynSent => {
                        if segment.rst() {
                            if seq_gt(segment.ack, connection.iss)
                                && seq_le(segment.ack, connection.snd_nxt)
                            {
                                close_error = Some(Errno::ECONNREFUSED);
                                closed = true;
                                self.counters.attempt_fails =
                                    self.counters.attempt_fails.wrapping_add(1);
                            }
                            break 'transition;
                        }
                        if segment.syn() && !segment.ack_flag() {
                            apply_syn_options(connection, segment, now);
                            connection.irs = segment.seq;
                            connection.rcv_nxt = segment.seq.wrapping_add(1);
                            connection.connect_deadline = None;
                            transition = Some(TcpState::SynReceived);
                            send_syn_ack = true;
                            break 'transition;
                        }
                        if !(seq_gt(segment.ack, connection.iss)
                            && seq_le(segment.ack, connection.snd_nxt))
                        {
                            send_rst = true;
                            rst_seq = segment.ack;
                            break 'transition;
                        }
                        if segment.syn() {
                            apply_syn_options(connection, segment, now);
                            connection.irs = segment.seq;
                            connection.rcv_nxt = segment.seq.wrapping_add(1);
                            connection.snd_una = segment.ack;
                            connection.snd_wnd = u32::from(segment.window);
                            connection.max_snd_wnd = connection.max_snd_wnd.max(segment.window);
                            connection.connect_deadline = None;
                            transition = Some(TcpState::Established);
                            send_ack = true;
                        }
                    }
                    TcpState::SynReceived => {
                        if segment.syn() && !segment.ack_flag() {
                            if segment.seq == connection.irs {
                                apply_syn_options(connection, segment, now);
                                send_syn_ack = true;
                            } else if seq_lt(segment.seq, connection.irs) {
                                // The LAND guard in `tcp_do_segment` resets a SYN
                                // below IRS instead of touching the half-open.
                                drop_with_reset = true;
                            } else {
                                send_ack = true;
                            }
                            break 'transition;
                        }
                        if segment.ack_flag()
                            && seq_gt(segment.ack, connection.snd_una)
                            && seq_le(segment.ack, connection.snd_nxt)
                        {
                            connection.snd_una = segment.ack;
                            // FreeBSD SYN_RECEIVED: snd_wnd = tiwin and
                            // snd_wl1 = SEG.SEQ - 1.  The following data segment
                            // is therefore the first one allowed to update the
                            // window, which keeps the split-1 stale-window rule.
                            let scaled = u32::from(segment.window) << connection.snd_wscale;
                            connection.snd_wnd = scaled;
                            connection.max_snd_wnd = connection
                                .max_snd_wnd
                                .max(u16::try_from(scaled).unwrap_or(u16::MAX));
                            connection.snd_wl1 = segment.seq.wrapping_sub(1);
                            connection.snd_wl2 = segment.ack;
                            transition = Some(TcpState::Established);
                            if !segment.payload.is_empty() {
                                incoming = Some((segment.seq, segment.payload));
                            }
                            if connection.passive {
                                pending_listener = Some(connection.local_port);
                            }
                        } else if segment.ack_flag() {
                            send_rst = true;
                            rst_seq = segment.ack;
                        }
                        if segment.fin() && segment.seq == connection.rcv_nxt {
                            if segment.payload.is_empty() {
                                connection.rcv_nxt = connection.rcv_nxt.wrapping_add(1);
                                connection.peer_fin = true;
                                transition = Some(TcpState::CloseWait);
                                send_ack = true;
                            } else {
                                fin_after_data = true;
                            }
                        }
                    }
                    TcpState::Established => {
                        if connection.timestamps
                            && let Some(tsval) = segment.tsval
                            && seq_lt(tsval, connection.ts_recent)
                            && now.saturating_since(connection.ts_recent_age).as_millis()
                                < 24 * 60 * 60 * 1000
                        {
                            connection.paws_drops = connection.paws_drops.saturating_add(1);
                            self.counters.paws_drops = self.counters.paws_drops.wrapping_add(1);
                            send_ack = true;
                            break 'transition;
                        }
                        if connection.timestamps
                            && let Some(tsval) = segment.tsval
                            && seq_gt(tsval, connection.ts_recent)
                            && seq_le(segment.seq, connection.rcv_nxt)
                        {
                            connection.ts_recent = tsval;
                            connection.ts_recent_age = now;
                        }
                        if !ack_window_ok(connection, segment.ack) {
                            if seq_gt(segment.ack, connection.highest_sent()) {
                                self.counters.acks_for_unsent =
                                    self.counters.acks_for_unsent.wrapping_add(1);
                            } else {
                                self.counters.ack_challenge =
                                    self.counters.ack_challenge.wrapping_add(1);
                            }
                            send_challenge = true;
                            break 'transition;
                        }
                        if segment.ack_flag() {
                            update_snd_wnd(connection, segment.seq, segment.ack, segment.window);
                        }
                        if segment.payload.is_empty()
                            && segment.ack_flag()
                            && segment.seq == connection.rcv_nxt.wrapping_sub(1)
                            && connection.keepalive_deadline.is_some()
                        {
                            self.counters.keepalive_probes_rcvd =
                                self.counters.keepalive_probes_rcvd.wrapping_add(1);
                        }
                        if connection.ecn_enabled
                            && connection.ecn_negotiated
                            && segment.flags.ece()
                        {
                            self.counters.ecn_ce_marks = self.counters.ecn_ce_marks.wrapping_add(1);
                            if !connection.ecn_cwr_pending {
                                connection.ecn_cwr_pending = true;
                                let flight = connection.snd_nxt.wrapping_sub(connection.snd_una);
                                let mut context = crate::CcContext {
                                    mss: u32::from(connection.mss),
                                    flight,
                                    pipe: flight,
                                    rtt: None,
                                    snd_nxt: connection.snd_nxt,
                                    snd_una: connection.snd_una,
                                };
                                connection
                                    .cc
                                    .on_loss(&mut context, crate::cc::LossKind::Ecn);
                                connection.recover_seq = connection.snd_nxt;
                                self.counters.ecn_ce_react =
                                    self.counters.ecn_ce_react.wrapping_add(1);
                            }
                        }
                        if seq_gt(segment.ack, connection.snd_una) {
                            ack_advanced = Some(segment.ack);
                            if connection.sack_recovery
                                && !connection.rto_recovery
                                && connection.timestamps
                                && let Some(echo) = segment.tsecr
                                && seq_lt(echo, connection.recovery_ts)
                            {
                                eifel_distance =
                                    Some(crate::sack_send::reorder_distance(connection));
                            }
                        }
                        if !segment.payload.is_empty() {
                            let right = connection
                                .rcv_nxt
                                .wrapping_add(u32::from(connection.rcv_wnd));
                            if !seq_ge(segment.seq, right) {
                                incoming = Some((segment.seq, segment.payload));
                            } else {
                                self.counters.past_window_drops =
                                    self.counters.past_window_drops.wrapping_add(1);
                                self.counters.past_window_bytes = self
                                    .counters
                                    .past_window_bytes
                                    .wrapping_add(segment.payload.len() as u64);
                                self.drops = self.drops.saturating_add(1);
                                send_ack = true;
                            }
                        }
                        let tiwin = u32::from(segment.window) << connection.snd_wscale;
                        let duplicate_window = segment.window <= connection.last_ack_window;
                        connection.last_ack_window = segment.window;
                        let sack_new = connection.sack_permitted
                            && segment.ack_flag()
                            && crate::sack_send::merge_sack_blocks(
                                connection,
                                &segment.sack_blocks,
                            );
                        let after_tlp = connection
                            .tlp_probe_at
                            .is_some_and(|probe| now.saturating_since(probe).as_millis() < 1000);
                        if !after_tlp
                            && seq_le(segment.ack, connection.snd_una)
                            && segment.payload.is_empty()
                            && connection.snd_una != connection.snd_nxt
                            && !segment.syn()
                            && !segment.fin()
                            && (tiwin == connection.snd_wnd || duplicate_window || sack_new)
                        {
                            connection.dupack_count = connection.dupack_count.saturating_add(1);
                            let threshold = u8::try_from(self.tunables.dup_ack_threshold)
                                .unwrap_or(3)
                                .max(
                                    u8::try_from(crate::sack_send::dup_threshold(connection))
                                        .unwrap_or(u8::MAX),
                                );
                            if connection.dupack_count >= threshold
                                || crate::sack_send::head_is_lost(connection)
                            {
                                connection.dupack_count = 0;
                                fast_retransmit = true;
                            }
                        } else if seq_gt(segment.ack, connection.snd_una) {
                            connection.dupack_count = 0;
                        }
                        if segment.fin() && segment.seq == connection.rcv_nxt {
                            if segment.payload.is_empty() {
                                connection.rcv_nxt = connection.rcv_nxt.wrapping_add(1);
                                connection.peer_fin = true;
                                transition = Some(TcpState::CloseWait);
                                send_ack = true;
                            } else {
                                fin_after_data = true;
                            }
                        }
                    }
                    TcpState::FinWait1 => {
                        if connection.timestamps
                            && let Some(tsval) = segment.tsval
                            && seq_lt(tsval, connection.ts_recent)
                            && now.saturating_since(connection.ts_recent_age).as_millis()
                                < 24 * 60 * 60 * 1000
                        {
                            connection.paws_drops = connection.paws_drops.saturating_add(1);
                            self.counters.paws_drops = self.counters.paws_drops.wrapping_add(1);
                            send_ack = true;
                            break 'transition;
                        }
                        if connection.timestamps
                            && let Some(tsval) = segment.tsval
                            && seq_gt(tsval, connection.ts_recent)
                            && seq_le(segment.seq, connection.rcv_nxt)
                        {
                            connection.ts_recent = tsval;
                            connection.ts_recent_age = now;
                        }
                        if !ack_window_ok(connection, segment.ack) {
                            if seq_gt(segment.ack, connection.snd_nxt) {
                                self.counters.acks_for_unsent =
                                    self.counters.acks_for_unsent.wrapping_add(1);
                            } else {
                                self.counters.ack_challenge =
                                    self.counters.ack_challenge.wrapping_add(1);
                            }
                            send_challenge = true;
                            break 'transition;
                        }
                        if segment.ack_flag() {
                            update_snd_wnd(connection, segment.seq, segment.ack, segment.window);
                        }
                        if !segment.payload.is_empty() {
                            incoming = Some((segment.seq, segment.payload));
                        }
                        if segment.ack == connection.snd_nxt {
                            connection.fin_acked = true;
                            if segment.fin() {
                                time_wait = true;
                                send_ack = true;
                            } else {
                                fin_wait2 = true;
                            }
                        }
                        if segment.fin() && segment.seq == connection.rcv_nxt {
                            connection.rcv_nxt = connection.rcv_nxt.wrapping_add(1);
                            connection.peer_fin = true;
                            if segment.ack == connection.snd_nxt {
                                time_wait = true;
                            } else {
                                transition = Some(TcpState::Closing);
                                send_ack = true;
                            }
                        }
                    }
                    TcpState::FinWait2 => {
                        if segment.ack_flag() {
                            update_snd_wnd(connection, segment.seq, segment.ack, segment.window);
                        }
                        if !segment.payload.is_empty() {
                            incoming = Some((segment.seq, segment.payload));
                        }
                        if segment.fin() && segment.seq == connection.rcv_nxt {
                            connection.rcv_nxt = connection.rcv_nxt.wrapping_add(1);
                            connection.peer_fin = true;
                            time_wait = true;
                            send_ack = true;
                        }
                    }
                    TcpState::CloseWait => {
                        if !segment.payload.is_empty() {
                            incoming = Some((segment.seq, segment.payload));
                        }
                        if segment.fin() {
                            send_ack = true;
                        }
                        if segment.ack_flag() {
                            update_snd_wnd(connection, segment.seq, segment.ack, segment.window);
                            // `tcp_input` advances `snd_una` and drops acknowledged bytes in
                            // every synchronized state. CLOSE_WAIT still has queued output
                            // after the peer's FIN, so without this the send queue never
                            // drains and the deferred FIN is never emitted.
                            if seq_gt(segment.ack, connection.snd_una) {
                                ack_advanced = Some(segment.ack);
                            }
                        }
                    }
                    TcpState::Closing => {
                        if segment.ack_flag() {
                            update_snd_wnd(connection, segment.seq, segment.ack, segment.window);
                        }
                        if !segment.payload.is_empty() {
                            self.counters.data_after_fin =
                                self.counters.data_after_fin.wrapping_add(1);
                        }
                        if segment.ack == connection.snd_nxt {
                            time_wait = true;
                        }
                        if segment.fin() {
                            send_ack = true;
                        }
                    }
                    TcpState::LastAck => {
                        if segment.ack_flag() {
                            update_snd_wnd(connection, segment.seq, segment.ack, segment.window);
                        }
                        if !segment.payload.is_empty() {
                            self.counters.data_after_fin =
                                self.counters.data_after_fin.wrapping_add(1);
                        }
                        if segment.ack == connection.snd_nxt {
                            closed = true;
                        } else if segment.ack_flag() {
                            send_challenge = true;
                        }
                    }
                    TcpState::TimeWait => {
                        if segment.rst() {
                            self.counters.timewait_rst_ignored =
                                self.counters.timewait_rst_ignored.wrapping_add(1);
                            break 'transition;
                        }
                        if segment.syn() {
                            let rfc6191 = segment.tsval.is_some_and(|tsval| {
                                connection.timestamps
                                    && (connection.ts_recent.wrapping_sub(tsval) as i32) < 0
                            });
                            if rfc6191 || seq_gt(segment.seq, connection.rcv_nxt) {
                                connection.irs = segment.seq;
                                connection.rcv_nxt = segment.seq.wrapping_add(1);
                                let iss = connection.iss.wrapping_add(0x0100_0000);
                                connection.iss = iss;
                                connection.snd_una = iss;
                                connection.snd_nxt = iss.wrapping_add(1);
                                connection.state = TcpState::SynReceived;
                                connection.time_wait_deadline = None;
                                send_ack = true;
                            } else {
                                self.counters.timewait_syn_drops =
                                    self.counters.timewait_syn_drops.wrapping_add(1);
                            }
                            break 'transition;
                        }
                        if !segment.payload.is_empty() {
                            self.counters.data_after_fin =
                                self.counters.data_after_fin.wrapping_add(1);
                            self.counters.timewait_data_drops =
                                self.counters.timewait_data_drops.wrapping_add(1);
                        }
                        if segment.fin() {
                            connection.rcv_nxt = connection.rcv_nxt.wrapping_add(1);
                            time_wait_refreshed = true;
                            connection.time_wait_deadline = Some(Instant::from_millis(
                                now.as_millis().saturating_add(
                                    i64::try_from(self.tunables.time_wait_interval)
                                        .unwrap_or(i64::MAX),
                                ),
                            ));
                            send_ack = true;
                        } else {
                            send_ack = true;
                        }
                    }
                    TcpState::Listen | TcpState::Closed => {}
                }
            }
            if let Some(state) = transition {
                connection.state = state;
            }
            if time_wait {
                connection.state = TcpState::TimeWait;
                connection.time_wait_deadline =
                    Some(Instant::from_millis(now.as_millis().saturating_add(
                        i64::try_from(self.tunables.time_wait_interval).unwrap_or(i64::MAX),
                    )));
            }
            if fin_wait2 {
                connection.state = TcpState::FinWait2;
                connection.fin_wait2_deadline =
                    Some(Instant::from_millis(now.as_millis().saturating_add(
                        i64::try_from(self.tunables.finwait2_timeout).unwrap_or(i64::MAX),
                    )));
            }
            if closed {
                connection.state = TcpState::Closed;
            }
            if let Some(errno) = close_error {
                connection.state = TcpState::Closed;
                record.error = Some(errno);
            }
            record.closed = closed;
        }
        if let Some(port) = pending_listener {
            let listener = self.sockets.records.iter().find_map(|(raw_id, record)| {
                if record.closed {
                    return None;
                }
                match record.kind {
                    Kind::Listener { port: bound, .. } if bound == port => Some(SocketId(*raw_id)),
                    _ => None,
                }
            });
            let entry = self.sockets.record(socket).ok().and_then(|record| {
                let Kind::Tcp(connection) = &record.kind else {
                    return None;
                };
                let (remote, remote_port) = connection.remote?;
                Some(crate::HalfOpen {
                    remote,
                    remote_port,
                    irs: connection.irs,
                    iss: connection.iss,
                    socket: socket.0,
                })
            });
            if let Some(listener) = listener
                && let Some(entry) = entry
                && let Ok(record) = self.sockets.record_mut(listener)
                && let Kind::Listener {
                    pending, half_open, ..
                } = &mut record.kind
            {
                half_open.remove_socket(socket.0);
                let limit = usize::try_from(self.tunables.accept_queue_limit).unwrap_or(128);
                if pending.len() >= limit {
                    pending.count_drop();
                    self.counters.accept_queue_drops =
                        self.counters.accept_queue_drops.wrapping_add(1);
                } else if !pending.contains_socket(socket.0) {
                    pending.push(entry);
                }
            }
        }
        if send_challenge {
            if self.allow_challenge(socket, now) {
                let _ = self.send_challenge_ack(socket);
            } else {
                self.counters.challenge_ack_suppressed =
                    self.counters.challenge_ack_suppressed.wrapping_add(1);
            }
        }
        if let Some((seq, data)) = incoming {
            self.tcp_deliver_data(socket, seq, data, now);
        }
        if fin_after_data {
            if let Ok(record) = self.sockets.record_mut(socket)
                && let Kind::Tcp(connection) = &mut record.kind
            {
                connection.rcv_nxt = connection.rcv_nxt.wrapping_add(1);
                connection.peer_fin = true;
                connection.state = TcpState::CloseWait;
            }
            let _ = self.send_ack_for(socket);
        }
        if let Some(ack) = ack_advanced {
            let _ = self.tcp_ack_data(socket, ack);
            if let Some(distance) = eifel_distance {
                self.undo_spurious_recovery(socket, distance);
            }
            if let Some(tsecr) = segment.tsecr
                && let Ok(record) = self.sockets.record_mut(socket)
                && let Kind::Tcp(connection) = &mut record.kind
                && let Some((tsval, sent_at)) = connection.last_ts_sent
                && tsval == tsecr
            {
                let rtt = self.now.as_millis().saturating_sub(sent_at.as_millis());
                if rtt >= 0 {
                    connection.rto.sample(u64::try_from(rtt).unwrap_or(0));
                }
                connection.last_ts_sent = None;
            }
        }
        if fast_retransmit {
            self.fast_retransmit(socket);
        }
        self.tcp_sack_recover(socket);
        if time_wait || time_wait_refreshed {
            self.note_time_wait(socket);
        }
        if send_syn_ack {
            let _ = self.send_syn_ack(socket);
        }
        if send_ack {
            let _ = self.send_ack_for(socket);
        }
        if send_rst {
            let _ = self.send_rst_for(socket, rst_seq);
        }
        if drop_with_reset {
            let _ = self.send_drop_with_reset(socket, segment);
        }
    }

    fn cookie_slot(&self, now_ms: i64) -> i64 {
        let lifetime = i64::try_from(self.tunables.syn_cookie_lifetime)
            .unwrap_or(60_000)
            .max(1);
        now_ms / lifetime
    }

    fn syn_cookie(
        &self,
        local: IpAddr,
        remote: IpAddr,
        local_port: u16,
        remote_port: u16,
        slot: i64,
    ) -> u32 {
        let mut hash = self.cookie_secret ^ (slot as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        for output in [local, remote] {
            let bytes = match output {
                IpAddr::V4(address) => address.octets().to_vec(),
                IpAddr::V6(address) => address.octets().to_vec(),
            };
            for byte in bytes {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        }
        hash ^= u64::from(local_port) << 16 | u64::from(remote_port);
        hash ^= hash >> 33;
        hash = hash.wrapping_mul(0xff51_afd7_ed55_8ccd);
        hash ^= hash >> 33;
        u32::try_from(hash & 0xffff_ffff).unwrap_or(0)
    }

    fn syn_cookie_valid(
        &self,
        local: IpAddr,
        remote: IpAddr,
        local_port: u16,
        remote_port: u16,
        acknowledgement: u32,
    ) -> bool {
        let slot = self.cookie_slot(self.now.as_millis());
        for candidate in [slot, slot.saturating_sub(1)] {
            let iss = self.syn_cookie(local, remote, local_port, remote_port, candidate);
            if acknowledgement == iss.wrapping_add(1) {
                return true;
            }
        }
        false
    }

    #[allow(clippy::too_many_arguments)]
    fn finish_syn_cookie(
        &mut self,
        listener: SocketId,
        source: IpAddr,
        destination: IpAddr,
        source_port: u16,
        destination_port: u16,
        segment: &Segment<'_>,
    ) {
        let iss = segment.ack.wrapping_sub(1);
        let mut connection = TcpConnection::new_passive(
            destination,
            destination_port,
            (source, source_port),
            iss,
            segment.seq.wrapping_sub(1),
            self.now,
        );
        connection.state = TcpState::Established;
        connection.snd_una = segment.ack;
        connection.snd_nxt = segment.ack;
        connection.snd_wnd = u32::from(segment.window);
        self.configure_tcp_connection(&mut connection, source);
        let Ok(socket) = self.sockets.alloc(
            Kind::Tcp(connection),
            self.recv_buffer_cap(),
            self.send_buffer_cap(),
        ) else {
            return;
        };
        self.counters.syn_cookie_valid = self.counters.syn_cookie_valid.wrapping_add(1);
        self.counters.passive_opens = self.counters.passive_opens.wrapping_add(1);
        if let Ok(record) = self.sockets.record_mut(listener)
            && let Kind::Listener { pending, .. } = &mut record.kind
        {
            let limit = usize::try_from(self.tunables.accept_queue_limit).unwrap_or(128);
            if pending.len() >= limit {
                pending.count_drop();
                self.counters.accept_queue_drops = self.counters.accept_queue_drops.wrapping_add(1);
            } else {
                pending.push(crate::HalfOpen {
                    remote: source,
                    remote_port: source_port,
                    irs: segment.seq.wrapping_sub(1),
                    iss: segment.ack.wrapping_sub(1),
                    socket: socket.0,
                });
            }
        }
        let _ = self.send_ack_for(socket);
    }

    fn fast_retransmit(&mut self, socket: SocketId) {
        let snapshot = {
            let Ok(record) = self.sockets.record(socket) else {
                return;
            };
            let Kind::Tcp(connection) = &record.kind else {
                return;
            };
            let Some(remote) = connection.remote else {
                return;
            };
            if connection.sack_recovery || connection.cc.in_recovery() {
                return;
            }
            let length = record.tx.len().min(usize::from(connection.mss));
            if length == 0 {
                return;
            }
            let payload: Vec<u8> = record.tx.iter().take(length).copied().collect();
            (
                connection.local,
                remote,
                connection.local_port,
                connection.snd_una,
                connection.rcv_nxt,
                advertised_window(record),
                payload,
                connection.snd_nxt,
                connection.mss,
            )
        };
        let (local, remote, port, seq, ack, window, payload, snd_nxt, mss) = snapshot;
        let now_ms = u32::try_from(self.now.as_millis()).unwrap_or(u32::MAX);
        let retransmit_options = self
            .sockets
            .record(socket)
            .ok()
            .and_then(|record| match &record.kind {
                Kind::Tcp(connection) => Some(timestamp_options(connection, now_ms)),
                _ => None,
            })
            .unwrap_or_default();
        if !retransmit_options.is_empty() {
            self.option_override = Some(retransmit_options);
        }
        let _ = self.send_tcp_packet(
            local,
            remote.0,
            port,
            remote.1,
            TcpFlags::from_bits(TcpFlags::ACK.bits() | TcpFlags::PSH.bits()),
            seq,
            ack,
            window,
            &payload,
            false,
        );
        self.counters.fast_retransmits = self.counters.fast_retransmits.wrapping_add(1);
        self.counters.retrans_segments = self.counters.retrans_segments.wrapping_add(1);
        self.counters.retrans_bytes = self
            .counters
            .retrans_bytes
            .wrapping_add(u64::try_from(payload.len()).unwrap_or(0));
        if let Ok(record) = self.sockets.record_mut(socket)
            && let Kind::Tcp(connection) = &mut record.kind
        {
            let flight = snd_nxt.wrapping_sub(seq);
            let mut context = crate::CcContext {
                mss: u32::from(mss),
                flight,
                pipe: flight,
                rtt: None,
                snd_nxt,
                snd_una: seq,
            };
            let saved = crate::recovery::SpuriousUndo::record(
                connection.cc.cwnd(),
                connection.cc.ssthresh(),
            );
            connection
                .cc
                .on_loss(&mut context, crate::cc::LossKind::DupAck);
            connection.undo = Some(saved);
            connection.recovery_ts = now_ms;
            connection.sack_recovery = true;
            connection.rto_recovery = false;
            connection.recover_seq = snd_nxt;
            connection.high_rxt = seq.wrapping_add(u32::try_from(payload.len()).unwrap_or(0));
            if crate::plpmtud_live::plpmtud_on_retransmit(
                connection,
                seq,
                u32::try_from(payload.len()).unwrap_or(0),
            ) {
                self.counters.plpmtud_losses = self.counters.plpmtud_losses.wrapping_add(1);
            }
            connection.retransmits_total = connection.retransmits_total.saturating_add(1);
            connection.rto_deadline =
                Some(Instant::from_millis(self.now.as_millis().saturating_add(
                    i64::try_from(connection.rto.rto_ms()).unwrap_or(i64::MAX),
                )));
        }
    }

    /// Record a connection in the TIME-WAIT table and recycle the oldest when it is full.
    fn note_time_wait(&mut self, socket: SocketId) {
        let Some((key, deadline)) = self.sockets.record(socket).ok().and_then(|record| {
            let Kind::Tcp(connection) = &record.kind else {
                return None;
            };
            if connection.state != TcpState::TimeWait {
                return None;
            }
            Some((
                crate::socket::connection_key(connection)?,
                connection.time_wait_deadline?.as_millis(),
            ))
        }) else {
            return;
        };
        let slot = crate::table::TimeWaitSlot {
            socket: socket.0,
            info: None,
        };
        let recycled = self.timewait.insert(key, slot, deadline);
        self.close_recycled(socket.0, &recycled);
    }

    /// Close the records of TIME-WAIT entries that were recycled to make room.
    fn close_recycled(&mut self, keep: u32, recycled: &[crate::table::TimeWaitSlot]) {
        for slot in recycled {
            self.counters.timewait_recycled = self.counters.timewait_recycled.wrapping_add(1);
            if slot.info.is_some() || slot.socket == keep {
                continue;
            }
            if let Some(record) = self.sockets.records.get_mut(&slot.socket)
                && let Kind::Tcp(connection) = &mut record.kind
                && connection.state == TcpState::TimeWait
            {
                connection.state = TcpState::Closed;
                record.closed = true;
            }
        }
    }

    /// Take in TIME-WAIT connections whose socket records were reclaimed.
    pub(crate) fn absorb_detached(&mut self) {
        for (key, info, deadline) in self.sockets.take_detached() {
            let slot = crate::table::TimeWaitSlot {
                socket: 0,
                info: Some(info),
            };
            let recycled = self.timewait.insert(key, slot, deadline);
            self.close_recycled(0, &recycled);
        }
    }

    /// Answer a segment for a TIME-WAIT connection whose record is gone.
    ///
    /// Returns true when the segment was consumed; false when it is a new incarnation (a SYN
    /// beyond the old receive sequence) that must be treated as a fresh connection attempt.
    fn detached_time_wait_input(
        &mut self,
        key: &crate::table::ConnectionKey,
        info: crate::table::TimeWaitInfo,
        segment: &Segment<'_>,
        now: Instant,
    ) -> bool {
        if segment.rst() {
            self.counters.timewait_rst_ignored = self.counters.timewait_rst_ignored.wrapping_add(1);
            return true;
        }
        if segment.syn() {
            if seq_gt(segment.seq, info.irs) {
                self.timewait.remove(key);
                return false;
            }
            self.counters.timewait_syn_drops = self.counters.timewait_syn_drops.wrapping_add(1);
            return true;
        }
        if !segment.payload.is_empty() {
            self.counters.data_after_fin = self.counters.data_after_fin.wrapping_add(1);
            self.counters.timewait_data_drops = self.counters.timewait_data_drops.wrapping_add(1);
        }
        if segment.fin() {
            let deadline = now.as_millis().saturating_add(
                i64::try_from(self.tunables.time_wait_interval).unwrap_or(i64::MAX),
            );
            let slot = crate::table::TimeWaitSlot {
                socket: 0,
                info: Some(info),
            };
            let recycled = self.timewait.insert(*key, slot, deadline);
            self.close_recycled(0, &recycled);
        }
        if info.timestamps {
            let mut options = Vec::with_capacity(10);
            options.extend_from_slice(&[8, 10]);
            options.extend_from_slice(&u32::try_from(now.as_millis()).unwrap_or(0).to_be_bytes());
            options.extend_from_slice(&info.ts_recent.to_be_bytes());
            self.option_override = Some(options);
        }
        let _ = self.send_tcp_packet(
            key.local,
            key.remote,
            key.local_port,
            key.remote_port,
            TcpFlags::ACK,
            info.snd_nxt,
            info.rcv_nxt,
            info.window,
            &[],
            false,
        );
        true
    }

    fn send_syn(&mut self, socket: SocketId) -> Result<(), Errno> {
        let (local, remote, iss, port, options) = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            let remote = connection.remote.ok_or(Errno::ENOTCONN)?;
            let tsval = u32::try_from(self.now.as_millis()).unwrap_or(0);
            let options = syn_options(connection, tsval, connection.ts_recent, true);
            (
                connection.local,
                remote,
                connection.iss,
                connection.local_port,
                options,
            )
        };
        self.option_override = Some(options);
        let flags = if self.tunables.ecn_enable != 0 {
            flags_or(TcpFlags::SYN, flags_or(TcpFlags::ECE, TcpFlags::CWR))
        } else {
            TcpFlags::SYN
        };
        self.send_tcp_packet(
            local,
            remote.0,
            port,
            remote.1,
            flags,
            iss,
            0,
            65_535,
            &[],
            true,
        )
    }

    pub(crate) fn send_syn_ack(&mut self, socket: SocketId) -> Result<(), Errno> {
        let (local, remote, iss, rcv_nxt, port, window, options) = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            let remote = connection.remote.ok_or(Errno::ENOTCONN)?;
            (
                connection.local,
                remote,
                connection.iss,
                connection.rcv_nxt,
                connection.local_port,
                connection.rcv_wnd,
                {
                    let tsval = u32::try_from(self.now.as_millis()).unwrap_or(0);
                    syn_options(
                        connection,
                        tsval,
                        connection.ts_recent,
                        connection.negotiated_wscale.is_some(),
                    )
                },
            )
        };
        self.option_override = Some(options);
        #[cfg(any(test, feature = "testutil"))]
        {
            self.tcp_synacks = self.tcp_synacks.saturating_add(1);
        }
        if let Ok(record) = self.sockets.record_mut(socket)
            && let Kind::Tcp(connection) = &mut record.kind
        {
            connection.last_ack_sent = rcv_nxt;
            if connection.state == TcpState::SynReceived {
                connection.rto_deadline =
                    Some(Instant::from_millis(self.now.as_millis().saturating_add(
                        i64::try_from(connection.rto.rto_ms()).unwrap_or(i64::MAX),
                    )));
            }
        }
        let mut flags = flags_or(TcpFlags::SYN, TcpFlags::ACK);
        if self.tunables.ecn_enable != 0
            && let Ok(record) = self.sockets.record(socket)
            && let Kind::Tcp(connection) = &record.kind
            && connection.ecn_negotiated
        {
            flags = flags_or(flags, TcpFlags::ECE);
        }
        self.send_tcp_packet(
            local,
            remote.0,
            port,
            remote.1,
            flags,
            iss,
            rcv_nxt,
            window,
            &[],
            true,
        )
    }

    pub(crate) fn send_ack_for(&mut self, socket: SocketId) -> Result<(), Errno> {
        let (options, window, previous_window, mss, ece) = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            let mut options =
                timestamp_options(connection, u32::try_from(self.now.as_millis()).unwrap_or(0));
            if connection.sack_permitted && !connection.ooo.is_empty() {
                let max_ranges = usize::try_from(self.tunables.sack_ranges_max).unwrap_or(16);
                let holes: Vec<(u32, u32)> = connection
                    .ooo
                    .iter()
                    .take(max_ranges)
                    .map(|(seq, data)| {
                        (
                            *seq,
                            seq.wrapping_add(u32::try_from(data.len()).unwrap_or(0)),
                        )
                    })
                    .collect();
                let blocks = crate::sack::receiver_blocks(&holes, connection.timestamps, None);
                if !blocks.is_empty() {
                    self.counters.sack_blocks_sent = self
                        .counters
                        .sack_blocks_sent
                        .wrapping_add(blocks.len() as u64);
                    options.extend_from_slice(&crate::sack::sack_option_bytes(&blocks));
                }
            }
            (
                options,
                advertised_window(record),
                connection.last_adv_window,
                connection.mss,
                connection.ecn_ece_pending,
            )
        };
        if !options.is_empty() {
            self.option_override = Some(options);
        }
        let threshold = mss.max(1);
        let grew = (window > previous_window
            && window.saturating_sub(previous_window) >= threshold)
            || (previous_window == 0 && window > 0);
        if grew {
            self.counters.window_updates = self.counters.window_updates.wrapping_add(1);
        }
        let (local, remote, snd_nxt, rcv_nxt, port) = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            let remote = connection.remote.ok_or(Errno::ENOTCONN)?;
            (
                connection.local,
                remote,
                connection.snd_nxt,
                connection.rcv_nxt,
                connection.local_port,
            )
        };
        if let Ok(record) = self.sockets.record_mut(socket)
            && let Kind::Tcp(connection) = &mut record.kind
        {
            connection.last_adv_window = window;
            connection.last_ack_sent = rcv_nxt;
        }
        let mut flags = TcpFlags::ACK;
        if ece {
            flags = flags_or(flags, TcpFlags::ECE);
        }
        self.send_tcp_packet(
            local,
            remote.0,
            port,
            remote.1,
            flags,
            snd_nxt,
            rcv_nxt,
            window,
            &[],
            false,
        )
    }

    fn send_challenge_ack(&mut self, socket: SocketId) -> Result<(), Errno> {
        self.counters.challenge_acks_sent = self.counters.challenge_acks_sent.wrapping_add(1);
        let (local, remote, snd_nxt, rcv_nxt, port, window) = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            let remote = connection.remote.ok_or(Errno::ENOTCONN)?;
            (
                connection.local,
                remote,
                connection.snd_nxt,
                connection.rcv_nxt,
                connection.local_port,
                advertised_window(record),
            )
        };
        if let Ok(record) = self.sockets.record_mut(socket)
            && let Kind::Tcp(connection) = &mut record.kind
        {
            connection.last_ack_sent = rcv_nxt;
        }
        self.send_tcp_packet(
            local,
            remote.0,
            port,
            remote.1,
            TcpFlags::ACK,
            snd_nxt,
            rcv_nxt,
            window,
            &[],
            false,
        )
    }

    fn send_rst_for(&mut self, socket: SocketId, seq: u32) -> Result<(), Errno> {
        let (local, remote, port) = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            let remote = connection.remote.ok_or(Errno::ENOTCONN)?;
            (connection.local, remote, connection.local_port)
        };
        self.send_tcp_packet(
            local,
            remote.0,
            port,
            remote.1,
            TcpFlags::RST,
            seq,
            0,
            0,
            &[],
            false,
        )
    }

    /// FreeBSD `tcp_dropwithreset`: an ACKed segment gets a bare RST echoing its
    /// ACK; a segment without ACK gets RST|ACK acknowledging its sequence space.
    fn send_drop_with_reset(
        &mut self,
        socket: SocketId,
        segment: &Segment<'_>,
    ) -> Result<(), Errno> {
        let (local, remote, port) = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            let remote = connection.remote.ok_or(Errno::ENOTCONN)?;
            (connection.local, remote, connection.local_port)
        };
        let (flags, seq, ack) = if segment.ack_flag() {
            (TcpFlags::RST, segment.ack, 0)
        } else {
            (
                flags_or(TcpFlags::RST, TcpFlags::ACK),
                0,
                segment.seq.wrapping_add(segment.len()),
            )
        };
        self.send_tcp_packet(
            local,
            remote.0,
            port,
            remote.1,
            flags,
            seq,
            ack,
            0,
            &[],
            false,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn send_tcp_packet(
        &mut self,
        local: IpAddr,
        remote: IpAddr,
        local_port: u16,
        remote_port: u16,
        flags: TcpFlags,
        seq: u32,
        ack: u32,
        window: u16,
        payload: &[u8],
        options: bool,
    ) -> Result<(), Errno> {
        let header = TcpHeader {
            src_port: local_port,
            dst_port: remote_port,
            sequence: seq,
            acknowledgement: ack,
            flags,
            window,
            urgent_pointer: 0,
        };
        let default_mss = [2u8, 4, 5, 0xb4];
        #[allow(unused_mut)]
        let mut option_bytes: Vec<u8> = if let Some(options) = self.option_override.take() {
            options
        } else if options {
            default_mss.to_vec()
        } else {
            Vec::new()
        };
        #[cfg(feature = "tcp-md5")]
        let md5_key = self.md5_keys.get(&local_port).cloned();
        #[cfg(feature = "tcp-md5")]
        if md5_key.is_some() {
            option_bytes.extend_from_slice(&[19, 18]);
            option_bytes.extend_from_slice(&[0u8; 16]);
        }
        #[cfg(feature = "tcp-ao")]
        let ao_key = self
            .ao_keys
            .get(&local_port)
            .and_then(|keys| keys.first())
            .cloned();
        #[cfg(feature = "tcp-ao")]
        if let Some(key) = &ao_key {
            let mac_len = match key.algorithm {
                crate::signature::AoAlgorithm::HmacSha1 => 20,
                crate::signature::AoAlgorithm::HmacSha256 => 32,
            };
            option_bytes.push(29);
            option_bytes.push(u8::try_from(4 + mac_len).unwrap_or(40));
            option_bytes.push(key.key_id);
            option_bytes.push(0);
            option_bytes.extend_from_slice(&vec![0u8; mac_len]);
        }
        let padded_options = (option_bytes.len() + 3) & !3;
        #[allow(unused_mut)]
        let mut signed = false;
        #[cfg(feature = "tcp-md5")]
        {
            signed |= md5_key.is_some();
        }
        #[cfg(feature = "tcp-ao")]
        {
            signed |= ao_key.is_some();
        }
        if !signed
            && padded_options <= 40
            && let (IpAddr::V4(local_v4), IpAddr::V4(remote_v4)) = (local, remote)
        {
            let context = ChecksumContext::ipv4(to_wire4(local_v4), to_wire4(remote_v4));
            let length = 20 + padded_options + payload.len();
            let result = self.send_ipv4_built(length, remote_v4, 6, &mut |region| {
                region[..20 + padded_options].fill(0);
                if let Ok(mut builder) =
                    TcpBuilder::new(region, context, &header, &option_bytes, payload.len())
                {
                    builder.payload_mut().copy_from_slice(payload);
                    builder.finish();
                }
            });
            return self.account_tcp_send(result, flags);
        }
        let mut segment = vec![0u8; 20 + padded_options + payload.len()];
        let context = match (local, remote) {
            (IpAddr::V4(local), IpAddr::V4(remote)) => {
                ChecksumContext::ipv4(to_wire4(local), to_wire4(remote))
            }
            (IpAddr::V6(local), IpAddr::V6(remote)) => {
                ChecksumContext::ipv6(to_wire6(local), to_wire6(remote))
            }
            _ => return Err(Errno::EAFNOSUPPORT),
        };
        {
            let mut builder =
                TcpBuilder::new(&mut segment, context, &header, &option_bytes, payload.len())
                    .map_err(|_| Errno::EINVAL)?;
            builder.payload_mut().copy_from_slice(payload);
            builder.finish();
        }
        #[cfg(feature = "tcp-md5")]
        if let Some(key) = md5_key {
            let md5_offset = 20 + option_bytes.len().saturating_sub(18);
            segment[16] = 0;
            segment[17] = 0;
            if let Some(slot) = segment.get_mut(md5_offset + 2..md5_offset + 18) {
                slot.fill(0);
            }
            let mut input = signature_pseudo(local, remote, segment.len());
            input.extend_from_slice(&segment[..20]);
            input.extend_from_slice(payload);
            input.extend_from_slice(&key);
            let digest = crate::signature::md5(&input);
            if let Some(slot) = segment.get_mut(md5_offset + 2..md5_offset + 18) {
                slot.copy_from_slice(&digest);
            }
            let checksum = context.compute(6, &segment);
            if let Some(field) = segment.get_mut(16..18) {
                field.copy_from_slice(&checksum.to_be_bytes());
            }
        }
        #[cfg(feature = "tcp-ao")]
        if let Some(key) = &ao_key {
            let mac_len = match key.algorithm {
                crate::signature::AoAlgorithm::HmacSha1 => 20,
                crate::signature::AoAlgorithm::HmacSha256 => 32,
            };
            let offset = 20 + option_bytes.len().saturating_sub(4 + mac_len);
            segment[16] = 0;
            segment[17] = 0;
            if let Some(slot) = segment.get_mut(offset + 4..offset + 4 + mac_len) {
                slot.fill(0);
            }
            let mut input = signature_pseudo(local, remote, segment.len());
            input.extend_from_slice(&segment);
            input.extend_from_slice(&key.key);
            let digest: Vec<u8> = match key.algorithm {
                crate::signature::AoAlgorithm::HmacSha1 => {
                    crate::signature::hmac_sha1(&key.key, &input).to_vec()
                }
                crate::signature::AoAlgorithm::HmacSha256 => {
                    crate::signature::hmac_sha256(&key.key, &input).to_vec()
                }
            };
            if let Some(slot) = segment.get_mut(offset + 4..offset + 4 + digest.len()) {
                slot.copy_from_slice(&digest);
            }
            let checksum = context.compute(6, &segment);
            if let Some(field) = segment.get_mut(16..18) {
                field.copy_from_slice(&checksum.to_be_bytes());
            }
        }
        let result = match remote {
            IpAddr::V4(destination) => self.send_ipv4(&segment, destination, 6),
            IpAddr::V6(destination) => self.send_ipv6(&segment, destination, 6),
        };
        self.account_tcp_send(result, flags)
    }

    /// Count a transmitted (or failed) TCP segment and pass the result through.
    fn account_tcp_send(
        &mut self,
        result: Result<(), Errno>,
        flags: TcpFlags,
    ) -> Result<(), Errno> {
        if result.is_ok() {
            self.counters.out_segments = self.counters.out_segments.wrapping_add(1);
            if flags.rst() {
                self.counters.rst_sent = self.counters.rst_sent.wrapping_add(1);
            }
        } else {
            self.counters.out_errors = self.counters.out_errors.wrapping_add(1);
        }
        result
    }

    #[allow(clippy::too_many_arguments)]
    fn send_tcp_raw(
        &mut self,
        local: IpAddr,
        remote: IpAddr,
        local_port: u16,
        remote_port: u16,
        flags: TcpFlags,
        seq: u32,
        ack: u32,
    ) -> Result<(), Errno> {
        self.send_tcp_packet(
            local,
            remote,
            local_port,
            remote_port,
            flags,
            seq,
            ack,
            0,
            &[],
            false,
        )
    }

    fn allow_challenge(&mut self, socket: SocketId, now: Instant) -> bool {
        let global_elapsed = now.saturating_since(self.challenge_global_last).as_millis();
        if global_elapsed >= self.tunables.challenge_ack_global_interval {
            self.challenge_global_tokens =
                u32::try_from(self.tunables.challenge_ack_global_burst).unwrap_or(u32::MAX);
            self.challenge_global_last = now;
        }
        if self.challenge_global_tokens == 0 {
            return false;
        }
        let burst = u32::try_from(self.tunables.challenge_ack_burst).unwrap_or(u32::MAX);
        let interval = self.tunables.challenge_ack_interval;
        let Ok(record) = self.sockets.record_mut(socket) else {
            return false;
        };
        let Kind::Tcp(connection) = &mut record.kind else {
            return false;
        };
        let elapsed = now.saturating_since(connection.challenge_last).as_millis();
        if elapsed >= interval {
            connection.challenge_tokens = burst;
            connection.challenge_last = now;
        }
        if connection.challenge_tokens == 0 {
            return false;
        }
        connection.challenge_tokens -= 1;
        self.challenge_global_tokens -= 1;
        true
    }

    pub(crate) fn tcp_shutdown(&mut self, socket: SocketId) -> Result<(), Errno> {
        let (state, seq, ack, local, remote, remote_port, port, window, tx_empty) = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            let remote = connection.remote.ok_or(Errno::ENOTCONN)?;
            (
                connection.state,
                connection.snd_nxt,
                connection.rcv_nxt,
                connection.local,
                remote.0,
                remote.1,
                connection.local_port,
                connection.rcv_wnd,
                record.tx.is_empty(),
            )
        };
        match state {
            TcpState::Closed => Err(Errno::EBADF),
            TcpState::SynSent => {
                let record = self.sockets.record_mut(socket)?;
                if let Kind::Tcp(connection) = &mut record.kind {
                    connection.state = TcpState::Closed;
                }
                record.closed = true;
                Ok(())
            }
            TcpState::SynReceived | TcpState::Established if tx_empty => {
                let _ = self.send_tcp_packet(
                    local,
                    remote,
                    port,
                    remote_port,
                    flags_or(TcpFlags::FIN, TcpFlags::ACK),
                    seq,
                    ack,
                    window,
                    &[],
                    false,
                );
                let record = self.sockets.record_mut(socket)?;
                if let Kind::Tcp(connection) = &mut record.kind {
                    connection.snd_nxt = connection.snd_nxt.wrapping_add(1);
                    connection.fin_sent = true;
                    connection.state = TcpState::FinWait1;
                }
                Ok(())
            }
            TcpState::SynReceived | TcpState::Established => {
                let record = self.sockets.record_mut(socket)?;
                if let Kind::Tcp(connection) = &mut record.kind {
                    connection.close_requested = true;
                }
                Ok(())
            }
            TcpState::CloseWait if tx_empty => {
                let _ = self.send_tcp_packet(
                    local,
                    remote,
                    port,
                    remote_port,
                    flags_or(TcpFlags::FIN, TcpFlags::ACK),
                    seq,
                    ack,
                    window,
                    &[],
                    false,
                );
                let record = self.sockets.record_mut(socket)?;
                if let Kind::Tcp(connection) = &mut record.kind {
                    connection.snd_nxt = connection.snd_nxt.wrapping_add(1);
                    connection.fin_sent = true;
                    connection.state = TcpState::LastAck;
                }
                Ok(())
            }
            TcpState::CloseWait => {
                // FreeBSD `tcp_usrclosed` in CLOSE_WAIT sets `TF_NEEDFIN`; `tcp_output`
                // emits the FIN only after `so_snd` drains. Sending the FIN at `snd_nxt`
                // here would place it ahead of the queued output, so the peer receives
                // one byte then EOF and loses the rest.
                let record = self.sockets.record_mut(socket)?;
                if let Kind::Tcp(connection) = &mut record.kind {
                    connection.close_requested = true;
                }
                Ok(())
            }
            TcpState::FinWait1
            | TcpState::FinWait2
            | TcpState::Closing
            | TcpState::LastAck
            | TcpState::TimeWait => Ok(()),
            TcpState::Listen => Err(Errno::ENOTCONN),
        }
    }

    pub(crate) fn process_tcp_timers(&mut self, now: Instant) {
        self.absorb_detached();
        let now_ms = u64::try_from(now.as_millis()).unwrap_or(0);
        for action in self.syncache.poll(now_ms) {
            if let crate::syncache::SyncacheAction::Retransmit(key) = action {
                let _ = self.send_syncache_syn_ack(&key);
            }
        }
        for (key, slot) in self.timewait.expire_entries(now.as_millis()) {
            if slot.info.is_some() {
                self.ports.release(key.local_port);
            }
        }
        let sockets: Vec<SocketId> = self
            .sockets
            .records
            .iter()
            .filter_map(|(raw_id, record)| match record.kind {
                Kind::Tcp(_) if !record.closed => Some(SocketId(*raw_id)),
                _ => None,
            })
            .collect();
        for socket in sockets {
            let mut retransmit_syn = false;
            let mut timeout = None;
            let mut close_time_wait = false;
            let mut fin_wait2_timeout = false;
            let mut keepalive_probe = false;
            let mut keepalive_timeout = false;
            let mut retransmit: Option<Retransmit> = None;
            let mut plp_resend = false;
            let mut resend_synack = false;
            let mut rack_loss = false;
            {
                let Ok(record) = self.sockets.record_mut(socket) else {
                    continue;
                };
                let advertised = advertised_window(record);
                let Kind::Tcp(connection) = &mut record.kind else {
                    continue;
                };
                if connection.state == TcpState::SynSent
                    && connection
                        .connect_deadline
                        .is_some_and(|deadline| deadline <= now)
                {
                    if u32::from(connection.connect_retries)
                        >= u32::try_from(self.tunables.connect_retries).unwrap_or(6)
                    {
                        timeout = Some(Errno::ETIMEDOUT);
                        self.counters.attempt_fails = self.counters.attempt_fails.wrapping_add(1);
                    } else {
                        connection.connect_retries = connection.connect_retries.saturating_add(1);
                        let backoff = 1000u64
                            .saturating_mul(1u64 << u32::from(connection.connect_retries.min(6)));
                        connection.connect_deadline = Some(Instant::from_millis(
                            now.as_millis()
                                .saturating_add(i64::try_from(backoff).unwrap_or(i64::MAX)),
                        ));
                        retransmit_syn = true;
                    }
                }
                if connection.state == TcpState::FinWait2
                    && connection
                        .fin_wait2_deadline
                        .is_some_and(|deadline| deadline <= now)
                {
                    fin_wait2_timeout = true;
                    self.counters.finwait2_fires = self.counters.finwait2_fires.wrapping_add(1);
                }
                if connection.state == TcpState::TimeWait
                    && connection
                        .time_wait_deadline
                        .is_some_and(|deadline| deadline <= now)
                {
                    close_time_wait = true;
                    self.counters.timewait_fires = self.counters.timewait_fires.wrapping_add(1);
                    self.counters.timewait_expiries =
                        self.counters.timewait_expiries.wrapping_add(1);
                }
                if connection.state == TcpState::SynReceived
                    && connection
                        .rto_deadline
                        .is_some_and(|deadline| deadline <= now)
                {
                    if connection.retransmit_count >= connection.rto_max_retransmits {
                        connection.state = TcpState::Closed;
                        connection.rto_deadline = None;
                        record.closed = true;
                        self.counters.attempt_fails = self.counters.attempt_fails.wrapping_add(1);
                    } else {
                        connection.retransmit_count = connection.retransmit_count.saturating_add(1);
                        connection.rto.timeout();
                        resend_synack = true;
                    }
                }
                if connection
                    .rto_deadline
                    .is_some_and(|deadline| deadline <= now)
                    && connection.plp_probe.is_some()
                    && crate::plpmtud_live::plpmtud_on_timeout(connection, now.as_millis())
                {
                    plp_resend = true;
                    self.counters.plpmtud_losses = self.counters.plpmtud_losses.wrapping_add(1);
                }
                if self.tunables.plpmtud_enable != 0
                    && connection
                        .rto_deadline
                        .is_some_and(|deadline| deadline <= now)
                    && connection.state == TcpState::Established
                    && connection.snd_nxt != connection.snd_una
                    && connection.retransmit_count.saturating_add(1)
                        >= crate::plpmtud_live::BLACKHOLE_TIMEOUTS
                    && crate::plpmtud_live::plpmtud_blackhole(connection, now.as_millis())
                {
                    let flight = connection.snd_nxt.wrapping_sub(connection.snd_una);
                    let mut context = crate::CcContext {
                        mss: u32::from(connection.mss),
                        flight,
                        pipe: flight,
                        rtt: None,
                        snd_nxt: connection.snd_nxt,
                        snd_una: connection.snd_una,
                    };
                    connection.cc.on_rto(&mut context);
                    connection.rto.timeout();
                    connection.retransmit_count = connection.retransmit_count.saturating_add(1);
                    connection.rewind_for_resend();
                    connection.rto_deadline = None;
                    connection.sacked.clear();
                    connection.sack_recovery = false;
                    plp_resend = true;
                    self.counters.plpmtud_losses = self.counters.plpmtud_losses.wrapping_add(1);
                }
                if connection
                    .rto_deadline
                    .is_some_and(|deadline| deadline <= now)
                    && connection.snd_nxt != connection.snd_una
                    && !matches!(connection.state, TcpState::Closed | TcpState::TimeWait)
                {
                    let length = record.tx.len().min(usize::from(connection.mss));
                    if length > 0 {
                        if let Some(sample) = &mut connection.rtt_sample {
                            sample.2 = true;
                        } else {
                            connection.rtt_sample = Some((connection.snd_una, now, true));
                        }
                        connection.retransmit_count = connection.retransmit_count.saturating_add(1);
                        connection.retransmits_total =
                            connection.retransmits_total.saturating_add(1);
                        connection.rto_timeouts = connection.rto_timeouts.saturating_add(1);
                        self.counters.rto_fires = self.counters.rto_fires.wrapping_add(1);
                        self.counters.retrans_segments =
                            self.counters.retrans_segments.wrapping_add(1);
                        self.counters.retrans_bytes = self
                            .counters
                            .retrans_bytes
                            .wrapping_add(u64::try_from(length).unwrap_or(0));
                        if connection.retransmit_count > connection.rto_max_retransmits {
                            connection.state = TcpState::Closed;
                            record.error = Some(Errno::ETIMEDOUT);
                            record.closed = true;
                            connection.rto_deadline = None;
                        } else {
                            let payload: Vec<u8> = record.tx.iter().take(length).copied().collect();
                            connection.last_retransmit_seq = Some(connection.snd_una);
                            if let Some(remote) = connection.remote {
                                retransmit = Some((
                                    payload,
                                    connection.local,
                                    remote,
                                    connection.local_port,
                                    connection.snd_una,
                                    connection.rcv_nxt,
                                    advertised,
                                ));
                            }
                            let flight = connection.snd_nxt.wrapping_sub(connection.snd_una);
                            let mut context = crate::CcContext {
                                mss: u32::from(connection.mss),
                                flight,
                                pipe: flight,
                                rtt: None,
                                snd_nxt: connection.snd_nxt,
                                snd_una: connection.snd_una,
                            };
                            connection.cc.on_rto(&mut context);
                            connection.recover_seq = connection.snd_nxt;
                            connection.sack_recovery = true;
                            connection.rto_recovery = true;
                            connection.dupack_count = 0;
                            connection.high_rxt = connection
                                .snd_una
                                .wrapping_add(u32::try_from(length).unwrap_or(0));
                            connection.tlp.disarm();
                            connection.rto.timeout();
                            connection.rto_deadline =
                                Some(Instant::from_millis(now.as_millis().saturating_add(
                                    i64::try_from(connection.rto.rto_ms()).unwrap_or(i64::MAX),
                                )));
                        }
                    }
                }
                if retransmit.is_none()
                    && connection.snd_una != connection.snd_nxt
                    && matches!(
                        connection.state,
                        TcpState::Established | TcpState::CloseWait | TcpState::FinWait1
                    )
                {
                    let now_ms = u32::try_from(now.as_millis()).unwrap_or(u32::MAX);
                    let length = record.tx.len().min(usize::from(connection.mss));
                    if length > 0 {
                        let srtt = u32::try_from(connection.rto.srtt_ms()).unwrap_or(u32::MAX);
                        let rto_deadline = connection.rto_deadline.map(|deadline| {
                            u32::try_from(deadline.as_millis()).unwrap_or(u32::MAX)
                        });
                        let tlp_due = connection.tlp_enabled
                            && !connection.sack_recovery
                            && connection.tlp.fires(now_ms)
                            && connection.tlp.is_sooner_than(rto_deadline);
                        if tlp_due {
                            let payload: Vec<u8> = record.tx.iter().take(length).copied().collect();
                            connection.tlp.disarm();
                            connection.tlp_probe_at = Some(now);
                            self.counters.tlp_fires = self.counters.tlp_fires.wrapping_add(1);
                            self.counters.retrans_segments =
                                self.counters.retrans_segments.wrapping_add(1);
                            self.counters.retrans_bytes = self
                                .counters
                                .retrans_bytes
                                .wrapping_add(u64::try_from(length).unwrap_or(0));
                            connection.retransmits_total =
                                connection.retransmits_total.saturating_add(1);
                            if let Some(remote) = connection.remote {
                                retransmit = Some((
                                    payload,
                                    connection.local,
                                    remote,
                                    connection.local_port,
                                    connection.snd_una,
                                    connection.rcv_nxt,
                                    advertised,
                                ));
                            }
                        } else if usize::from(connection.dupack_count)
                            >= usize::try_from(crate::sack_send::dup_threshold(connection))
                                .unwrap_or(usize::MAX)
                            && !connection.sack_recovery
                            && let Some((_end_seq, sent_at)) = connection.rack_sent.first().copied()
                        {
                            let bound = srtt
                                .saturating_mul(2)
                                .max(connection.rack.reorder_window.saturating_mul(2))
                                .max(20);
                            if now_ms.saturating_sub(sent_at) > bound {
                                connection.rack_sent.remove(0);
                                connection.dupack_count = 0;
                                rack_loss = true;
                            }
                        }
                    }
                }
                if connection.state == TcpState::Established
                    && self.tunables.user_timeout > 0
                    && now.saturating_since(connection.last_activity).as_millis()
                        >= self.tunables.user_timeout
                {
                    timeout = Some(Errno::ETIMEDOUT);
                    self.counters.user_timeouts = self.counters.user_timeouts.wrapping_add(1);
                }
                if connection.state == TcpState::Established
                    && connection
                        .keepalive_deadline
                        .is_some_and(|deadline| deadline <= now)
                {
                    if connection.keepalive_left == 0 {
                        keepalive_timeout = true;
                    } else {
                        connection.keepalive_left = connection.keepalive_left.saturating_sub(1);
                        connection.keepalive_deadline = Some(Instant::from_millis(
                            now.as_millis().saturating_add(
                                i64::try_from(connection.keepalive_probe_interval_ms)
                                    .unwrap_or(i64::MAX),
                            ),
                        ));
                        keepalive_probe = true;
                    }
                }
                if let Some(errno) = timeout {
                    connection.state = TcpState::Closed;
                    record.error = Some(errno);
                    record.closed = true;
                }
                if fin_wait2_timeout {
                    connection.state = TcpState::Closed;
                    record.error = Some(Errno::ETIMEDOUT);
                    record.closed = true;
                    self.counters.finwait2_timeouts =
                        self.counters.finwait2_timeouts.wrapping_add(1);
                }
                if keepalive_timeout {
                    connection.state = TcpState::Closed;
                    record.error = Some(Errno::ETIMEDOUT);
                    record.closed = true;
                }
                if close_time_wait {
                    connection.state = TcpState::Closed;
                    record.closed = true;
                }
            }
            if retransmit_syn {
                self.counters.connect_fires = self.counters.connect_fires.wrapping_add(1);
                let _ = self.send_syn(socket);
            }
            if keepalive_probe {
                self.counters.keepalive_fires = self.counters.keepalive_fires.wrapping_add(1);
                let _ = self.send_ack_for(socket);
            }
            if let Some((payload, local, remote, local_port, seq, ack, window)) = retransmit {
                let _ = self.send_tcp_packet(
                    local,
                    remote.0,
                    local_port,
                    remote.1,
                    TcpFlags::from_bits(TcpFlags::ACK.bits() | TcpFlags::PSH.bits()),
                    seq,
                    ack,
                    window,
                    &payload,
                    false,
                );
            }
            if plp_resend {
                let _ = self.tcp_try_send(socket);
            }
            if resend_synack {
                let _ = self.send_syn_ack(socket);
            }
            if rack_loss {
                self.counters.rack_losses = self.counters.rack_losses.wrapping_add(1);
                self.fast_retransmit(socket);
            }
            self.tcp_delayed_ack(socket, now);
            let _ = self.tcp_zero_window_timer(socket, now);
        }
    }
}

/// FreeBSD `tcp_do_segment` step 6: only a newer segment/ACK may update `snd_wnd`.
fn update_snd_wnd(connection: &mut TcpConnection, seq: u32, ack: u32, window: u16) {
    let scaled = u32::from(window) << connection.snd_wscale;
    // A receiver advertising zero has stopped accepting data; stop sending immediately and
    // let the persist timer probe.  A stale zero is safe; a missed zero is a stall.
    if window == 0 && connection.snd_wnd != 0 {
        connection.snd_wnd = 0;
        connection.snd_wl1 = seq;
        connection.snd_wl2 = ack;
        return;
    }
    if seq_lt(connection.snd_wl1, seq)
        || (connection.snd_wl1 == seq
            && (seq_lt(connection.snd_wl2, ack)
                || (connection.snd_wl2 == ack && scaled > connection.snd_wnd)))
        || (window == 0 && !seq_lt(seq, connection.snd_wl1) && connection.snd_wnd != 0)
    {
        connection.snd_wnd = scaled;
        connection.snd_wl1 = seq;
        connection.snd_wl2 = ack;
        connection.max_snd_wnd = connection
            .max_snd_wnd
            .max(u16::try_from(scaled).unwrap_or(u16::MAX));
    }
}

enum ResetDisposition {
    Accept,
    Challenge,
    Ignore,
}

/// FreeBSD `tcp_do_segment` RFC 5961 section 3.2: a RST is accepted only at
/// `last_ack_sent` or `RCV.NXT`, challenged when it lands in the window, and
/// ignored otherwise.
fn reset_disposition(connection: &TcpConnection, seq: u32) -> ResetDisposition {
    let in_window = if connection.rcv_wnd == 0 {
        connection.last_ack_sent == seq || connection.rcv_nxt == seq
    } else {
        seq_ge(seq, connection.last_ack_sent)
            && seq_lt(
                seq,
                connection
                    .rcv_nxt
                    .wrapping_add(u32::from(connection.rcv_wnd)),
            )
    };
    if !in_window {
        return ResetDisposition::Ignore;
    }
    if connection.last_ack_sent == seq || connection.rcv_nxt == seq {
        ResetDisposition::Accept
    } else {
        ResetDisposition::Challenge
    }
}

fn ack_window_ok(connection: &TcpConnection, ack: u32) -> bool {
    if seq_gt(ack, connection.highest_sent()) {
        return false;
    }
    let lower = connection
        .snd_una
        .wrapping_sub(u32::from(connection.max_snd_wnd));
    seq_ge(ack, lower)
}

#[cfg(any(feature = "tcp-md5", feature = "tcp-ao"))]
#[allow(dead_code)]
fn find_option(segment: &[u8], kind: u8) -> Option<usize> {
    let header_len = usize::from(*segment.get(12)? >> 4) * 4;
    let mut offset = 20;
    while offset + 2 <= header_len && offset + 2 <= segment.len() {
        let option = *segment.get(offset)?;
        if option == 0 || option == 1 {
            offset += 1;
            continue;
        }
        let length = usize::from(*segment.get(offset + 1)?);
        if length < 2 {
            return None;
        }
        if option == kind {
            return Some(offset);
        }
        offset += length;
    }
    None
}

#[cfg(any(feature = "tcp-md5", feature = "tcp-ao"))]
fn signature_pseudo(local: IpAddr, remote: IpAddr, length: usize) -> Vec<u8> {
    match (local, remote) {
        (IpAddr::V4(local), IpAddr::V4(remote)) => raven_tcpip_wire::checksum::pseudo_header_v4(
            to_wire4(local),
            to_wire4(remote),
            6,
            u16::try_from(length).unwrap_or(u16::MAX),
        )
        .to_vec(),
        (IpAddr::V6(local), IpAddr::V6(remote)) => raven_tcpip_wire::checksum::pseudo_header_v6(
            to_wire6(local),
            to_wire6(remote),
            6,
            u32::try_from(length).unwrap_or(u32::MAX),
        )
        .to_vec(),
        _ => Vec::new(),
    }
}

fn syncache_address(address: IpAddr) -> SyncAddress {
    match address {
        IpAddr::V4(value) => SyncAddress::V4(value.octets()),
        IpAddr::V6(value) => SyncAddress::V6(value.octets()),
    }
}

fn syncache_key(local: IpAddr, local_port: u16, remote: IpAddr, remote_port: u16) -> SyncKey {
    SyncKey {
        local: SyncEndpoint {
            address: syncache_address(local),
            port: local_port,
        },
        remote: SyncEndpoint {
            address: syncache_address(remote),
            port: remote_port,
        },
    }
}

fn syncache_ip(address: SyncAddress) -> IpAddr {
    match address {
        SyncAddress::V4(octets) => IpAddr::V4(Ipv4Addr::new(octets)),
        SyncAddress::V6(octets) => IpAddr::V6(Ipv6Addr::new(octets)),
    }
}

fn choose_wscale(receive_cap: usize) -> u8 {
    let mut shift = 0u8;
    while shift < 14 && (65_535usize << shift) < receive_cap {
        shift += 1;
    }
    shift
}

fn our_mss(remote: IpAddr, caps: crate::DeviceCaps, default: u16) -> u16 {
    if caps.mtu == 0 {
        return default.max(64);
    }
    let link = caps.mtu.saturating_sub(14);
    let header = match remote {
        IpAddr::V4(_) => 40,
        IpAddr::V6(_) => 60,
    };
    u16::try_from(link.saturating_sub(header))
        .unwrap_or(default)
        .max(536)
}

fn apply_syn_options(connection: &mut TcpConnection, segment: &Segment<'_>, now: Instant) {
    if let Some(mss) = segment.mss {
        let clamped = mss.clamp(connection.mss_floor, connection.our_mss);
        if clamped != mss {
            connection.mss_clamps = connection.mss_clamps.saturating_add(1);
        }
        connection.peer_mss = clamped;
        connection.mss = connection.peer_mss.min(connection.our_mss);
    }
    if connection.offer_wscale
        && let Some(peer_scale) = segment.wscale
    {
        // RFC 7323 §2.2: scaling is negotiated only when the peer offered WS.
        // A plain SYN/ACK leaves snd_wscale at 0 and negotiated_wscale unset.
        connection.snd_wscale = peer_scale.min(14);
        connection.negotiated_wscale = Some(connection.snd_wscale);
    }
    if segment.sack_permitted && connection.offer_sack {
        connection.sack_permitted = true;
    }
    if connection.ecn_enabled && segment.flags.ece() {
        connection.ecn_negotiated = true;
        connection.ecn.negotiate(true, true);
    }
    if connection.offer_timestamps
        && let Some(tsval) = segment.tsval
    {
        connection.timestamps = true;
        connection.ts_recent = tsval;
        connection.ts_recent_age = now;
    }
    let option_reduction = if connection.timestamps { 12 } else { 0 };
    connection.mss = connection
        .peer_mss
        .min(connection.our_mss)
        .saturating_sub(option_reduction)
        .max(1);
}

fn syn_options(
    connection: &TcpConnection,
    tsval: u32,
    tsecr: u32,
    include_wscale: bool,
) -> Vec<u8> {
    let mut options = Vec::new();
    options.extend_from_slice(&[2, 4]);
    options.extend_from_slice(&connection.our_mss.to_be_bytes());
    if connection.offer_sack {
        options.extend_from_slice(&[4, 2]);
    }
    options.extend_from_slice(&[1, 1]);
    if connection.offer_timestamps {
        options.extend_from_slice(&[8, 10]);
        options.extend_from_slice(&tsval.to_be_bytes());
        options.extend_from_slice(&tsecr.to_be_bytes());
        options.extend_from_slice(&[1, 1]);
    }
    if connection.offer_wscale && include_wscale {
        options.extend_from_slice(&[3, 3, connection.rcv_wscale]);
    }
    options.push(0);
    options
}

pub(crate) fn timestamp_options_public(connection: &TcpConnection, tsval: u32) -> Vec<u8> {
    timestamp_options(connection, tsval)
}

fn timestamp_options(connection: &TcpConnection, tsval: u32) -> Vec<u8> {
    if !connection.timestamps {
        return Vec::new();
    }
    let mut options = Vec::new();
    options.extend_from_slice(&[8, 10]);
    options.extend_from_slice(&tsval.to_be_bytes());
    options.extend_from_slice(&connection.ts_recent.to_be_bytes());
    options
}

fn flags_or(first: TcpFlags, second: TcpFlags) -> TcpFlags {
    TcpFlags::from_bits(first.bits() | second.bits())
}

fn is_group(destination: IpAddr) -> bool {
    match destination {
        IpAddr::V4(address) => address.is_multicast() || address.is_broadcast(),
        IpAddr::V6(address) => address.is_multicast(),
    }
}

#[cfg(test)]
#[path = "tcp_state_ut.rs"]
mod tests;
