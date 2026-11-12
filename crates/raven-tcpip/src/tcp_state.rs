// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! TCP connection state machine, RFC 5961 hardening and lifecycle timers.

use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::tcp::{TcpBuilder, TcpFlags, TcpHeader, TcpPacket};

use crate::device::Device;
use crate::ipv4_stack::to_wire as to_wire4;
use crate::ipv6_stack::{IpAddr, Ipv6Addr, to_wire as to_wire6};
use crate::seq::{seq_ge, seq_gt, seq_le, seq_lt};
use crate::socket::{Kind, SocketRecord};
use crate::{Instant, Ipv4Addr, SocketId, Stack};

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
    pub snd_wnd: u16,
    pub max_snd_wnd: u16,
    pub rcv_nxt: u32,
    pub rcv_wnd: u16,
    #[allow(dead_code)] // Window-update ordering is consumed by C09's send path.
    pub snd_wl1: u32,
    #[allow(dead_code)]
    pub snd_wl2: u32,
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

struct Segment {
    seq: u32,
    ack: u32,
    flags: TcpFlags,
    window: u16,
    urgent: u16,
    payload: Vec<u8>,
}

impl Segment {
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

    /// Half-open connections of a listener (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_listener_half_open(&self, listener: SocketId) -> Vec<SocketId> {
        match self.sockets.record(listener).map(|record| &record.kind) {
            Ok(Kind::Listener { half_open, .. }) => half_open.iter().copied().collect(),
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

    /// TCP segments received (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_debug(&self) -> &[(u16, u16, u8, u32, u32)] {
        &self.tcp_debug
    }

    /// The current send window and in-flight bytes (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_tcp_window(&self, socket: SocketId) -> Option<(u16, u32, u32)> {
        match &self.sockets.record(socket).ok()?.kind {
            Kind::Tcp(connection) => {
                Some((connection.snd_wnd, connection.snd_una, connection.snd_nxt))
            }
            _ => None,
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
        let record = self.sockets.record_mut(socket)?;
        let Kind::Tcp(connection) = &mut record.kind else {
            return Err(Errno::ENOTSOCK);
        };
        connection.keepalive_interval_ms = interval.as_millis();
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
        let iss = self.next_isn();
        let connection = TcpConnection::new_active(local, local_port, remote, iss, self.now);
        let socket = self.sockets.alloc(
            Kind::Tcp(connection),
            self.config.rx_queue_bytes,
            self.config.tx_queue_bytes,
        )?;
        let _ = self.send_syn(socket);
        Ok(socket)
    }

    fn next_isn(&mut self) -> u32 {
        let iss = self.next_isn;
        self.next_isn = self.next_isn.wrapping_add(0x0100_0000);
        iss
    }

    pub(crate) fn tcp_input(
        &mut self,
        source: IpAddr,
        destination: IpAddr,
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
        let Ok(packet) = TcpPacket::parse(payload, context) else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        #[cfg(any(test, feature = "testutil"))]
        self.tcp_debug.push((
            packet.src_port(),
            packet.dst_port(),
            packet.flags().bits(),
            packet.sequence(),
            packet.acknowledgement(),
        ));
        let segment = Segment {
            seq: packet.sequence(),
            ack: packet.acknowledgement(),
            flags: packet.flags(),
            window: packet.window(),
            urgent: packet.urgent_pointer(),
            payload: packet.payload().to_vec(),
        };
        let source_port = packet.src_port();
        let destination_port = packet.dst_port();
        if segment.flags.urg() {
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

        let exact = self.sockets.records.iter().find_map(|(raw_id, record)| {
            if record.closed {
                return None;
            }
            match &record.kind {
                Kind::Tcp(connection)
                    if connection.local == destination
                        && connection.local_port == destination_port
                        && connection.remote == Some((source, source_port)) =>
                {
                    Some(SocketId(*raw_id))
                }
                _ => None,
            }
        });
        if let Some(socket) = exact {
            self.tcp_input_established(socket, &segment, now);
            return;
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

    #[allow(clippy::too_many_arguments)]
    fn tcp_listen_input(
        &mut self,
        listener: SocketId,
        source: IpAddr,
        destination: IpAddr,
        source_port: u16,
        destination_port: u16,
        segment: &Segment,
        now: Instant,
    ) {
        if segment.rst() {
            return;
        }
        if segment.ack_flag() {
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
        let backlog_full = match self.sockets.record(listener) {
            Ok(SocketRecord {
                kind:
                    Kind::Listener {
                        _backlog,
                        half_open,
                        ..
                    },
                ..
            }) => half_open.len() >= *_backlog,
            _ => return,
        };
        if backlog_full {
            self.drops = self.drops.saturating_add(1);
            return;
        }
        let iss = self.next_isn();
        let connection = TcpConnection::new_passive(
            destination,
            destination_port,
            (source, source_port),
            iss,
            segment.seq,
            now,
        );
        let Ok(socket) = self.sockets.alloc(
            Kind::Tcp(connection),
            self.config.rx_queue_bytes,
            self.config.tx_queue_bytes,
        ) else {
            return;
        };
        if let Ok(record) = self.sockets.record_mut(listener)
            && let Kind::Listener { half_open, .. } = &mut record.kind
        {
            half_open.push_back(socket);
        }
        let _ = self.send_syn_ack(socket);
    }

    #[allow(unused_assignments)]
    fn tcp_input_established(&mut self, socket: SocketId, segment: &Segment, now: Instant) {
        let mut transition = None;
        let mut send_ack = false;
        let mut send_challenge = false;
        let mut send_rst = false;
        let mut send_syn_ack = false;
        let mut rst_seq = 0;
        let mut close_error = None;
        let mut time_wait = false;
        let mut fin_wait2 = false;
        let mut closed = false;
        let mut pending_listener = None;
        let mut ack_advanced = None;
        let mut incoming: Option<(u32, Vec<u8>)> = None;
        let mut fin_after_data = false;
        {
            let Ok(record) = self.sockets.record_mut(socket) else {
                return;
            };
            let Kind::Tcp(connection) = &mut record.kind else {
                return;
            };
            'transition: {
                match connection.state {
                    TcpState::SynSent => {
                        if segment.rst() {
                            if seq_gt(segment.ack, connection.iss)
                                && seq_le(segment.ack, connection.snd_nxt)
                            {
                                close_error = Some(Errno::ECONNREFUSED);
                                closed = true;
                            }
                            break 'transition;
                        }
                        if segment.syn() && !segment.ack_flag() {
                            connection.irs = segment.seq;
                            connection.rcv_nxt = segment.seq.wrapping_add(1);
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
                            connection.irs = segment.seq;
                            connection.rcv_nxt = segment.seq.wrapping_add(1);
                            connection.snd_una = segment.ack;
                            connection.snd_wnd = segment.window;
                            connection.max_snd_wnd = connection.max_snd_wnd.max(segment.window);
                            transition = Some(TcpState::Established);
                            send_ack = true;
                        }
                    }
                    TcpState::SynReceived => {
                        if segment.rst() {
                            if segment.seq == connection.rcv_nxt {
                                if connection.passive {
                                    closed = true;
                                } else {
                                    close_error = Some(Errno::ECONNREFUSED);
                                    closed = true;
                                }
                            } else if in_window(connection, segment.seq) {
                                send_challenge = true;
                            }
                            break 'transition;
                        }
                        if segment.syn() && !segment.ack_flag() {
                            if segment.seq == connection.irs {
                                send_ack = true;
                            } else {
                                closed = true;
                            }
                            break 'transition;
                        }
                        if segment.ack_flag()
                            && seq_gt(segment.ack, connection.snd_una)
                            && seq_le(segment.ack, connection.snd_nxt)
                        {
                            connection.snd_una = segment.ack;
                            connection.snd_wnd = segment.window;
                            connection.max_snd_wnd = connection.max_snd_wnd.max(segment.window);
                            transition = Some(TcpState::Established);
                            if connection.passive {
                                pending_listener = Some(connection.local_port);
                            }
                        } else if segment.ack_flag() {
                            send_rst = true;
                            rst_seq = segment.ack;
                        }
                        if segment.fin() && segment.seq == connection.rcv_nxt {
                            connection.rcv_nxt = connection.rcv_nxt.wrapping_add(1);
                            connection.peer_fin = true;
                            transition = Some(TcpState::CloseWait);
                            send_ack = true;
                        }
                    }
                    TcpState::Established => {
                        if segment.rst() {
                            if segment.seq == connection.rcv_nxt {
                                close_error = Some(Errno::ECONNRESET);
                                closed = true;
                            } else if in_window(connection, segment.seq) {
                                send_challenge = true;
                            }
                            break 'transition;
                        }
                        if segment.syn() {
                            send_challenge = true;
                            break 'transition;
                        }
                        if !ack_window_ok(connection, segment.ack) {
                            send_challenge = true;
                            break 'transition;
                        }
                        if seq_gt(segment.ack, connection.snd_una) {
                            ack_advanced = Some(segment.ack);
                        }
                        if !segment.payload.is_empty() {
                            let right = connection
                                .rcv_nxt
                                .wrapping_add(u32::from(connection.rcv_wnd));
                            if !seq_ge(segment.seq, right) {
                                incoming = Some((segment.seq, segment.payload.clone()));
                            } else {
                                self.drops = self.drops.saturating_add(1);
                                send_ack = true;
                            }
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
                        if segment.rst() {
                            if segment.seq == connection.rcv_nxt {
                                close_error = Some(Errno::ECONNRESET);
                                closed = true;
                            } else if in_window(connection, segment.seq) {
                                send_challenge = true;
                            }
                            break 'transition;
                        }
                        if segment.syn() {
                            send_challenge = true;
                            break 'transition;
                        }
                        if !ack_window_ok(connection, segment.ack) {
                            send_challenge = true;
                            break 'transition;
                        }
                        if !segment.payload.is_empty() {
                            incoming = Some((segment.seq, segment.payload.clone()));
                        }
                        if segment.ack == connection.snd_nxt {
                            connection.fin_acked = true;
                            if segment.fin() {
                                time_wait = true;
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
                        if segment.rst() {
                            if segment.seq == connection.rcv_nxt {
                                close_error = Some(Errno::ECONNRESET);
                                closed = true;
                            } else if in_window(connection, segment.seq) {
                                send_challenge = true;
                            }
                            break 'transition;
                        }
                        if segment.syn() {
                            send_challenge = true;
                            break 'transition;
                        }
                        if !segment.payload.is_empty() {
                            incoming = Some((segment.seq, segment.payload.clone()));
                        }
                        if segment.fin() && segment.seq == connection.rcv_nxt {
                            connection.rcv_nxt = connection.rcv_nxt.wrapping_add(1);
                            connection.peer_fin = true;
                            time_wait = true;
                            send_ack = true;
                        }
                    }
                    TcpState::CloseWait => {
                        if segment.rst() {
                            if segment.seq == connection.rcv_nxt {
                                close_error = Some(Errno::ECONNRESET);
                                closed = true;
                            } else if in_window(connection, segment.seq) {
                                send_challenge = true;
                            }
                            break 'transition;
                        }
                        if segment.syn() {
                            send_challenge = true;
                            break 'transition;
                        }
                        if !segment.payload.is_empty() {
                            incoming = Some((segment.seq, segment.payload.clone()));
                        }
                        if segment.fin() {
                            send_ack = true;
                        }
                        if segment.ack_flag() {
                            connection.snd_wnd = segment.window;
                        }
                    }
                    TcpState::Closing => {
                        if segment.rst() {
                            if segment.seq == connection.rcv_nxt {
                                close_error = Some(Errno::ECONNRESET);
                                closed = true;
                            } else if in_window(connection, segment.seq) {
                                send_challenge = true;
                            }
                            break 'transition;
                        }
                        if segment.syn() {
                            send_challenge = true;
                            break 'transition;
                        }
                        if segment.ack == connection.snd_nxt {
                            time_wait = true;
                        }
                        if segment.fin() {
                            send_ack = true;
                        }
                    }
                    TcpState::LastAck => {
                        if segment.rst() {
                            if segment.seq == connection.rcv_nxt {
                                close_error = Some(Errno::ECONNRESET);
                                closed = true;
                            } else if in_window(connection, segment.seq) {
                                send_challenge = true;
                            }
                            break 'transition;
                        }
                        if segment.syn() {
                            send_challenge = true;
                            break 'transition;
                        }
                        if segment.ack == connection.snd_nxt {
                            closed = true;
                        } else if segment.ack_flag() {
                            send_challenge = true;
                        }
                    }
                    TcpState::TimeWait => {
                        if segment.rst() {
                            break 'transition;
                        }
                        if segment.syn() {
                            if seq_gt(segment.seq, connection.irs) {
                                connection.irs = segment.seq;
                                connection.rcv_nxt = segment.seq.wrapping_add(1);
                                let iss = connection.iss.wrapping_add(0x0100_0000);
                                connection.iss = iss;
                                connection.snd_una = iss;
                                connection.snd_nxt = iss.wrapping_add(1);
                                connection.state = TcpState::SynReceived;
                                connection.time_wait_deadline = None;
                                send_ack = true;
                            }
                            break 'transition;
                        }
                        if segment.fin() {
                            connection.rcv_nxt = connection.rcv_nxt.wrapping_add(1);
                            connection.time_wait_deadline =
                                Some(Instant::from_millis(now.as_millis().saturating_add(60_000)));
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
                    Some(Instant::from_millis(now.as_millis().saturating_add(60_000)));
            }
            if fin_wait2 {
                connection.state = TcpState::FinWait2;
                connection.fin_wait2_deadline =
                    Some(Instant::from_millis(now.as_millis().saturating_add(60_000)));
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
            if let Some(listener) = listener
                && let Ok(record) = self.sockets.record_mut(listener)
                && let Kind::Listener {
                    pending, half_open, ..
                } = &mut record.kind
            {
                half_open.retain(|connection| *connection != socket);
                if !pending.contains(&socket) {
                    pending.push_back(socket);
                }
            }
        }
        if send_challenge && self.allow_challenge(socket, now) {
            let _ = self.send_challenge_ack(socket);
        }
        if let Some((seq, data)) = incoming {
            self.tcp_deliver_data(socket, seq, &data, now);
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
    }

    fn send_syn(&mut self, socket: SocketId) -> Result<(), Errno> {
        let (local, remote, iss, port) = {
            let record = self.sockets.record(socket)?;
            let Kind::Tcp(connection) = &record.kind else {
                return Err(Errno::ENOTSOCK);
            };
            let remote = connection.remote.ok_or(Errno::ENOTCONN)?;
            (
                connection.local,
                remote,
                connection.iss,
                connection.local_port,
            )
        };
        self.send_tcp_packet(
            local,
            remote.0,
            port,
            remote.1,
            TcpFlags::SYN,
            iss,
            0,
            65_535,
            &[],
            true,
        )
    }

    pub(crate) fn send_syn_ack(&mut self, socket: SocketId) -> Result<(), Errno> {
        let (local, remote, iss, rcv_nxt, port, window) = {
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
            )
        };
        self.send_tcp_packet(
            local,
            remote.0,
            port,
            remote.1,
            flags_or(TcpFlags::SYN, TcpFlags::ACK),
            iss,
            rcv_nxt,
            window,
            &[],
            true,
        )
    }

    pub(crate) fn send_ack_for(&mut self, socket: SocketId) -> Result<(), Errno> {
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
                connection.rcv_wnd,
            )
        };
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

    fn send_challenge_ack(&mut self, socket: SocketId) -> Result<(), Errno> {
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
                connection.rcv_wnd,
            )
        };
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
        let mss = [2u8, 4, 5, 0xb4];
        let option_bytes: &[u8] = if options { &mss } else { &[] };
        let padded_options = (option_bytes.len() + 3) & !3;
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
                TcpBuilder::new(&mut segment, context, &header, option_bytes, payload.len())
                    .map_err(|_| Errno::EINVAL)?;
            builder.payload_mut().copy_from_slice(payload);
            builder.finish();
        }
        match remote {
            IpAddr::V4(destination) => self.send_ipv4(&segment, destination, 6),
            IpAddr::V6(destination) => self.send_ipv6(&segment, destination, 6),
        }
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
        let Ok(record) = self.sockets.record_mut(socket) else {
            return false;
        };
        let Kind::Tcp(connection) = &mut record.kind else {
            return false;
        };
        let elapsed = now.saturating_since(connection.challenge_last).as_millis();
        if elapsed >= 5_000 {
            connection.challenge_tokens = 10;
            connection.challenge_last = now;
        }
        if connection.challenge_tokens == 0 {
            return false;
        }
        connection.challenge_tokens -= 1;
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
            TcpState::CloseWait => {
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
            TcpState::FinWait1
            | TcpState::FinWait2
            | TcpState::Closing
            | TcpState::LastAck
            | TcpState::TimeWait => Ok(()),
            TcpState::Listen => Err(Errno::ENOTCONN),
        }
    }

    pub(crate) fn process_tcp_timers(&mut self, now: Instant) {
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
            {
                let Ok(record) = self.sockets.record_mut(socket) else {
                    continue;
                };
                let Kind::Tcp(connection) = &mut record.kind else {
                    continue;
                };
                if connection.state == TcpState::SynSent
                    && connection
                        .connect_deadline
                        .is_some_and(|deadline| deadline <= now)
                {
                    if connection.connect_retries >= 6 {
                        timeout = Some(Errno::ETIMEDOUT);
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
                }
                if connection.state == TcpState::TimeWait
                    && connection
                        .time_wait_deadline
                        .is_some_and(|deadline| deadline <= now)
                {
                    close_time_wait = true;
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
                        connection.keepalive_deadline =
                            Some(Instant::from_millis(now.as_millis().saturating_add(
                                i64::try_from(connection.keepalive_interval_ms).unwrap_or(i64::MAX),
                            )));
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
                let _ = self.send_syn(socket);
            }
            if keepalive_probe {
                let _ = self.send_ack_for(socket);
            }
            self.tcp_delayed_ack(socket, now);
            let _ = self.tcp_zero_window_timer(socket, now);
        }
    }
}

fn in_window(connection: &TcpConnection, seq: u32) -> bool {
    seq_ge(seq, connection.rcv_nxt)
        && seq_lt(
            seq,
            connection
                .rcv_nxt
                .wrapping_add(u32::from(connection.rcv_wnd)),
        )
}

fn ack_window_ok(connection: &TcpConnection, ack: u32) -> bool {
    if seq_gt(ack, connection.snd_nxt) {
        return false;
    }
    let lower = connection
        .snd_una
        .wrapping_sub(u32::from(connection.max_snd_wnd));
    seq_ge(ack, lower)
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
