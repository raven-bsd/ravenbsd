// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ICMPv4 echo handling, error delivery and rate-limited error generation.

use alloc::vec;
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_tcpip_wire::icmpv4::{Icmpv4Builder, Icmpv4Packet};
use raven_tcpip_wire::ipv4::{Ipv4Builder, Ipv4Header, Ipv4Packet};

use crate::device::Device;
use crate::ipv4_stack::to_wire;
use crate::socket::Kind;
use crate::{Instant, Ipv4Addr, SocketId, Stack};

impl<D: Device> Stack<D> {
    /// Send an ICMPv4 echo request.
    pub fn icmp_echo_request(
        &mut self,
        to: Ipv4Addr,
        id: u16,
        seq: u16,
        payload: &[u8],
    ) -> Result<(), Errno> {
        self.send_icmp_message(8, 0, u32::from(id) << 16 | u32::from(seq), payload, to)
    }

    /// Bind an ICMPv4 echo identifier; replies are delivered through [`Stack::recv`].
    pub fn icmp_bind_echo(&mut self, id: u16) -> Result<SocketId, Errno> {
        let duplicate = self.sockets.records.values().any(|record| {
            !record.closed && matches!(record.kind, Kind::Icmp { id: bound } if bound == id)
        });
        if duplicate {
            return Err(Errno::EADDRINUSE);
        }
        let rx_cap = self.config.rx_queue_bytes;
        let tx_cap = self.config.tx_queue_bytes;
        self.sockets.alloc(Kind::Icmp { id }, rx_cap, tx_cap)
    }

    pub(crate) fn handle_icmpv4(
        &mut self,
        source: Ipv4Addr,
        destination: Ipv4Addr,
        payload: &[u8],
    ) {
        let Ok(message) = Icmpv4Packet::parse(payload) else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        match message.message_type() {
            8 => {
                if destination.is_broadcast() || destination.is_multicast() {
                    return;
                }
                let _ = self.send_icmp_message(
                    0,
                    message.code(),
                    message.rest(),
                    message.payload(),
                    source,
                );
            }
            0 => self.deliver_echo_reply(&message),
            3 | 11 | 12 => self.handle_icmp_error(&message),
            5 => self.icmp_redirects = self.icmp_redirects.saturating_add(1),
            _ => {}
        }
    }

    fn deliver_echo_reply(&mut self, message: &Icmpv4Packet<'_>) {
        let id = message.id();
        let socket = self.sockets.records.iter().find_map(|(raw_id, record)| {
            if record.closed {
                return None;
            }
            match record.kind {
                Kind::Icmp { id: bound } if bound == id => Some(SocketId(*raw_id)),
                _ => None,
            }
        });
        let Some(socket) = socket else {
            return;
        };
        let Ok(record) = self.sockets.record_mut(socket) else {
            return;
        };
        if message.payload_len() > record.rx_cap {
            self.drops = self.drops.saturating_add(1);
            return;
        }
        record.rx.extend(message.payload().iter().copied());
    }

    fn handle_icmp_error(&mut self, message: &Icmpv4Packet<'_>) {
        let quoted = message.payload();
        let is_pmtu = message.message_type() == 3 && message.code() == 4;
        let Ok(inner) = Ipv4Packet::parse(quoted) else {
            if is_pmtu {
                self.counters.pmtu_invalid = self.counters.pmtu_invalid.wrapping_add(1);
            }
            return;
        };
        if inner.protocol() == 6 && is_pmtu {
            if inner.payload().len() < 4 || self.tunables.ignore_path_mtu != 0 {
                if inner.payload().len() < 4 {
                    self.counters.pmtu_invalid = self.counters.pmtu_invalid.wrapping_add(1);
                }
                return;
            }
            let mtu = message.rest().max(crate::pmtu::IPV4_FLOOR);
            let destination = crate::ipv4_stack::from_wire(inner.dst());
            self.pmtu4.insert(destination, (mtu, self.now));
            let max = usize::try_from(self.tunables.pmtu_cache_max).unwrap_or(1024);
            while self.pmtu4.len() > max {
                let Some(key) = self.pmtu4.keys().next().copied() else {
                    break;
                };
                self.pmtu4.remove(&key);
            }
            for record in self.sockets.records.values_mut() {
                if let Kind::Tcp(connection) = &mut record.kind
                    && connection.state != crate::TcpState::Closed
                    && connection
                        .remote
                        .is_some_and(|(address, _)| address == crate::IpAddr::V4(destination))
                {
                    let path = u16::try_from(mtu.saturating_sub(40)).unwrap_or(u16::MAX);
                    connection.mss = connection.our_mss.min(path).max(connection.mss_floor);
                }
            }
            self.counters.pmtu_updates = self.counters.pmtu_updates.wrapping_add(1);
            return;
        }
        if inner.protocol() != 17 {
            return;
        }
        let transport = inner.payload();
        let (Some(&source_hi), Some(&source_lo), Some(&dest_hi), Some(&dest_lo)) = (
            transport.first(),
            transport.get(1),
            transport.get(2),
            transport.get(3),
        ) else {
            return;
        };
        let _source_port = u16::from_be_bytes([source_hi, source_lo]);
        let destination_port = u16::from_be_bytes([dest_hi, dest_lo]);
        let error = match (message.message_type(), message.code()) {
            (3, 0) => Errno::ENETUNREACH,
            (3, 1) => Errno::EHOSTUNREACH,
            (3, 2) => Errno::EPROTO,
            (3, 3) => Errno::ECONNREFUSED,
            (3, 4) => Errno::EHOSTUNREACH,
            (11, _) => Errno::ETIMEDOUT,
            (12, _) => Errno::EPROTO,
            _ => Errno::EPROTO,
        };
        let socket = self.sockets.records.iter().find_map(|(raw_id, record)| {
            if record.closed {
                return None;
            }
            match &record.kind {
                Kind::Udp { local_port, .. } if *local_port == destination_port => {
                    Some(SocketId(*raw_id))
                }
                _ => None,
            }
        });
        if let Some(socket) = socket
            && let Ok(record) = self.sockets.record_mut(socket)
        {
            record.error = Some(error);
        }
    }

    pub(crate) fn send_port_unreachable(
        &mut self,
        source: Ipv4Addr,
        destination: Ipv4Addr,
        original: &[u8],
    ) {
        if destination.is_broadcast() || destination.is_multicast() {
            return;
        }
        if !self.allow_icmp_error(self.now) {
            self.drops = self.drops.saturating_add(1);
            return;
        }
        let Some(quote) = self.quote_datagram(destination, source, original) else {
            return;
        };
        if self.send_icmp_message(3, 3, 0, &quote, source).is_ok() {
            self.icmp_errors_sent = self.icmp_errors_sent.saturating_add(1);
        }
    }

    fn quote_datagram(
        &self,
        source: Ipv4Addr,
        destination: Ipv4Addr,
        original: &[u8],
    ) -> Option<Vec<u8>> {
        let quote_len = original.len().min(8);
        let header = Ipv4Header {
            ttl: 64,
            protocol: 17,
            src: to_wire(source),
            dst: to_wire(destination),
            ..Ipv4Header::default()
        };
        let mut packet = vec![0u8; 20 + quote_len];
        let mut builder = Ipv4Builder::new(&mut packet, &header, quote_len).ok()?;
        builder
            .payload_mut()
            .copy_from_slice(&original[..quote_len]);
        Some(packet)
    }

    fn send_icmp_message(
        &mut self,
        message_type: u8,
        code: u8,
        rest: u32,
        payload: &[u8],
        destination: Ipv4Addr,
    ) -> Result<(), Errno> {
        let mut message = vec![0u8; 8 + payload.len()];
        {
            let mut builder =
                Icmpv4Builder::new(&mut message, message_type, code, rest, payload.len())
                    .map_err(|_| Errno::EINVAL)?;
            builder.payload_mut().copy_from_slice(payload);
            builder.finish();
        }
        self.send_ipv4(&message, destination, 1)
    }

    pub(crate) fn allow_icmp_error(&mut self, now: Instant) -> bool {
        let elapsed = now.saturating_since(self.icmp_last_refill).as_millis();
        self.icmp_last_refill = now;
        let rate = u64::from(self.config.icmp_error_rate_pps);
        let burst = u64::from(self.config.icmp_error_burst);
        let refill = elapsed.saturating_mul(rate) / 1000;
        let tokens = (u64::from(self.icmp_tokens).saturating_add(refill)).min(burst);
        if tokens == 0 {
            self.icmp_tokens = 0;
            return false;
        }
        self.icmp_tokens = u32::try_from(tokens - 1).unwrap_or(0);
        true
    }

    /// Outbound ICMP errors sent.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_icmp_errors_sent(&self) -> usize {
        self.icmp_errors_sent
    }

    /// ICMP redirects observed and ignored.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_icmp_redirects(&self) -> usize {
        self.icmp_redirects
    }
}
