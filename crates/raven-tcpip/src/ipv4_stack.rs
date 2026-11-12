// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! ARP, IPv4 input/output, routing, fragmentation and reassembly.

use alloc::vec;
#[cfg(any(test, feature = "testutil"))]
use alloc::vec::Vec;

use raven_abi::errno::Errno;
use raven_tcpip_wire::arp::{ArpBuilder, ArpPacket};
use raven_tcpip_wire::ethernet::{EtherType, EthernetBuilder, EthernetFrame, MacAddr};
use raven_tcpip_wire::ipv4::{
    Ipv4Addr as WireIpv4Addr, Ipv4Builder, Ipv4Flags, Ipv4Header, Ipv4Packet,
};

use crate::device::{Device, Medium};
use crate::neighbor::{NeighborCache, QueuedPacket};
use crate::reassembly::Key;
use crate::route::{Route, RouteTable};
use crate::stack::{Ipv4Config, Stack};
use crate::{Instant, Ipv4Addr};

const ARP_LIFETIME_MS: u64 = 60_000;
const ARP_MAX_RETRIES: u8 = 3;
pub(crate) const REASSEMBLY_TIMEOUT_MS: u64 = 30_000;

fn to_wire(address: Ipv4Addr) -> WireIpv4Addr {
    WireIpv4Addr::new(address.octets())
}

fn from_wire(address: WireIpv4Addr) -> Ipv4Addr {
    Ipv4Addr::new(address.octets())
}

impl<D: Device> Stack<D> {
    /// Configure an IPv4 address and install the connected route plus an optional default route.
    pub fn set_ipv4(&mut self, config: Ipv4Config) -> Result<(), Errno> {
        if config.prefix_len > 32 {
            return Err(Errno::EINVAL);
        }
        if config.addr.is_unspecified() {
            return Err(Errno::EADDRNOTAVAIL);
        }
        let caps = self.device.capabilities();
        let mtu = match caps.medium {
            Medium::Ethernet => caps.mtu.saturating_sub(14),
            Medium::Ip => caps.mtu,
        }
        .max(68);
        self.ipv4 = Some(config);
        self.routes.clear();
        self.routes.add(Route {
            prefix: config.addr,
            prefix_len: config.prefix_len,
            gateway: None,
            mtu,
        })?;
        if let Some(gateway) = config.gateway {
            self.routes.add(Route {
                prefix: Ipv4Addr::UNSPECIFIED,
                prefix_len: 0,
                gateway: Some(gateway),
                mtu,
            })?;
        }
        Ok(())
    }

    /// The current IPv4 configuration.
    #[must_use]
    pub const fn ipv4(&self) -> Option<Ipv4Config> {
        self.ipv4
    }

    /// The route table.
    #[must_use]
    pub const fn routes(&self) -> &RouteTable {
        &self.routes
    }

    /// The route table, mutably.
    pub const fn routes_mut(&mut self) -> &mut RouteTable {
        &mut self.routes
    }

    /// The neighbour cache.
    #[must_use]
    pub const fn neighbors(&self) -> &NeighborCache {
        &self.neighbors
    }

    pub(crate) fn handle_frame(&mut self, frame: &[u8], now: Instant) {
        let Ok(ethernet) = EthernetFrame::parse(frame) else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        match ethernet.ether_type() {
            EtherType::Arp => self.handle_arp(ethernet.payload(), now),
            EtherType::Ipv4 => self.handle_ipv4(ethernet.payload(), now),
            EtherType::Ipv6 | EtherType::Unknown(_) => {}
        }
    }

    fn handle_arp(&mut self, payload: &[u8], now: Instant) {
        let Ok(packet) = ArpPacket::parse(payload) else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        let Some(config) = self.ipv4 else {
            return;
        };
        let sender = from_wire(packet.sender_protocol());
        let sender_mac = packet.sender_hardware().octets();
        let target = from_wire(packet.target_protocol());

        match packet.operation() {
            1 if target == config.addr => {
                let gratuitous = sender == target;
                if gratuitous {
                    if self.neighbors.get(sender).is_some() {
                        self.learn_neighbor(sender, sender_mac, now);
                    }
                    return;
                }
                if sender.is_unspecified() {
                    return;
                }
                self.learn_neighbor(sender, sender_mac, now);
                let _ = self.send_arp_reply(sender, sender_mac);
            }
            2 if target == config.addr && !sender.is_unspecified() => {
                self.learn_neighbor(sender, sender_mac, now);
            }
            _ => {}
        }
    }

    fn learn_neighbor(&mut self, address: Ipv4Addr, mac: [u8; 6], now: Instant) {
        let queued = self.neighbors.take_queued(address);
        self.neighbors
            .insert_resolved(address, mac, now, ARP_LIFETIME_MS);
        if let Some(packet) = queued {
            self.flush_queued(packet, mac);
        }
    }

    fn flush_queued(&mut self, packet: QueuedPacket, mac: [u8; 6]) {
        match self.routes.lookup(packet.destination).copied() {
            Some(route) => {
                if self
                    .transmit_fragments(
                        mac,
                        &route,
                        &packet.payload,
                        packet.destination,
                        packet.protocol,
                    )
                    .is_err()
                {
                    self.drops = self.drops.saturating_add(1);
                    self.ip_errors
                        .push((packet.destination, Errno::EHOSTUNREACH));
                }
            }
            None => {
                self.drops = self.drops.saturating_add(1);
                self.ip_errors
                    .push((packet.destination, Errno::ENETUNREACH));
            }
        }
    }

    fn handle_ipv4(&mut self, payload: &[u8], now: Instant) {
        let Ok(packet) = Ipv4Packet::parse(payload) else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        let Some(config) = self.ipv4 else {
            return;
        };
        let destination = from_wire(packet.dst());
        if destination != config.addr && !packet.dst().is_broadcast() {
            self.drops = self.drops.saturating_add(1);
            return;
        }
        let source = from_wire(packet.src());
        if packet.is_fragment() {
            let key = Key {
                src: source,
                dst: destination,
                id: packet.id(),
                protocol: packet.protocol(),
            };
            match self.reassembly.insert(
                key,
                usize::from(packet.frag_offset()),
                packet.flags().mf(),
                packet.payload(),
                now,
            ) {
                Ok(Some(datagram)) => self.deliver_ipv4(packet.protocol(), &datagram),
                Ok(None) => {}
                Err(_) => self.drops = self.drops.saturating_add(1),
            }
            return;
        }
        self.deliver_ipv4(packet.protocol(), packet.payload());
    }

    fn deliver_ipv4(&mut self, protocol: u8, payload: &[u8]) {
        #[cfg(any(test, feature = "testutil"))]
        self.delivered.push((protocol, payload.to_vec()));
        #[cfg(not(any(test, feature = "testutil")))]
        {
            let _ = (protocol, payload);
        }
    }

    #[allow(dead_code)] // Called by the transport tickets and the testutil hook.
    pub(crate) fn send_ipv4(
        &mut self,
        payload: &[u8],
        destination: Ipv4Addr,
        protocol: u8,
    ) -> Result<(), Errno> {
        let config = self.ipv4.ok_or(Errno::ENETUNREACH)?;
        let route = *self.routes.lookup(destination).ok_or(Errno::ENETUNREACH)?;
        let next_hop = route.gateway.unwrap_or(destination);
        if next_hop == config.addr {
            self.deliver_ipv4(protocol, payload);
            return Ok(());
        }
        let resolved = self
            .neighbors
            .get(next_hop)
            .filter(|entry| entry.is_resolved(self.now))
            .and_then(|entry| entry.mac);
        if let Some(mac) = resolved {
            return self.transmit_fragments(mac, &route, payload, destination, protocol);
        }
        if self
            .neighbors
            .get(next_hop)
            .is_some_and(|entry| entry.has_queued())
        {
            self.drops = self.drops.saturating_add(1);
            return Err(Errno::EAGAIN);
        }
        let deadline = Instant::from_millis(
            self.now
                .as_millis()
                .saturating_add(i64::try_from(1000u64).unwrap_or(i64::MAX)),
        );
        self.neighbors
            .pending(
                next_hop,
                QueuedPacket {
                    destination,
                    protocol,
                    payload: payload.to_vec(),
                },
                deadline,
            )
            .map_err(|_| Errno::ENOBUFS)?;
        self.send_arp_request(next_hop)?;
        Ok(())
    }

    fn transmit_fragments(
        &mut self,
        mac: [u8; 6],
        route: &Route,
        payload: &[u8],
        destination: Ipv4Addr,
        protocol: u8,
    ) -> Result<(), Errno> {
        let config = self.ipv4.ok_or(Errno::ENETUNREACH)?;
        let mtu = route.mtu.max(68);
        let max_payload = mtu.saturating_sub(20);
        if max_payload < 8 {
            return Err(Errno::EINVAL);
        }
        let chunk_size = max_payload & !7;
        let id = self.next_ip_id;
        self.next_ip_id = self.next_ip_id.wrapping_add(1);
        let mut offset = 0usize;
        loop {
            let remaining = payload.len().saturating_sub(offset);
            let take = remaining.min(chunk_size);
            let more = offset.saturating_add(take) < payload.len();
            let flags = if more {
                Ipv4Flags::MF
            } else {
                Ipv4Flags::default()
            };
            let header = Ipv4Header {
                id,
                flags,
                frag_offset: u16::try_from(offset).map_err(|_| Errno::EINVAL)?,
                ttl: 64,
                protocol,
                src: to_wire(config.addr),
                dst: to_wire(destination),
                ..Ipv4Header::default()
            };
            let mut packet = vec![0u8; 20 + take];
            {
                let mut builder =
                    Ipv4Builder::new(&mut packet, &header, take).map_err(|_| Errno::EINVAL)?;
                builder
                    .payload_mut()
                    .copy_from_slice(&payload[offset..offset + take]);
            }
            self.transmit_ethernet(MacAddr::new(mac), EtherType::Ipv4, &packet)?;
            if !more {
                break;
            }
            offset = offset.saturating_add(take);
        }
        Ok(())
    }

    fn transmit_ethernet(
        &mut self,
        destination: MacAddr,
        ether_type: EtherType,
        payload: &[u8],
    ) -> Result<(), Errno> {
        let caps = self.device.capabilities();
        let source = self
            .config
            .hardware_addr
            .or(caps.hardware_addr)
            .ok_or(Errno::ENETUNREACH)?;
        let total = 14usize.saturating_add(payload.len());
        if total > caps.mtu {
            return Err(Errno::EINVAL);
        }
        let mut frame = vec![0u8; total];
        {
            let mut builder = EthernetBuilder::new(
                &mut frame,
                destination,
                MacAddr::new(source),
                ether_type,
                payload.len(),
            )
            .map_err(|_| Errno::EINVAL)?;
            builder.payload_mut().copy_from_slice(payload);
        }
        let now = self.now;
        self.device
            .transmit(now, total, &mut |buffer| buffer.copy_from_slice(&frame))
            .map_err(|error| match error {
                crate::TxError::Busy => Errno::EAGAIN,
                crate::TxError::TooLong => Errno::EINVAL,
                crate::TxError::Down => Errno::ENETUNREACH,
            })?;
        self.pending_tx_frames = self.pending_tx_frames.saturating_add(1);
        Ok(())
    }

    fn send_arp_request(&mut self, target: Ipv4Addr) -> Result<(), Errno> {
        let config = self.ipv4.ok_or(Errno::ENETUNREACH)?;
        let caps = self.device.capabilities();
        let source = self
            .config
            .hardware_addr
            .or(caps.hardware_addr)
            .ok_or(Errno::ENETUNREACH)?;
        let mut packet = [0u8; 28];
        {
            let builder = ArpBuilder::new(
                &mut packet,
                1,
                MacAddr::new(source),
                to_wire(config.addr),
                MacAddr::new([0; 6]),
                to_wire(target),
            )
            .map_err(|_| Errno::EINVAL)?;
            builder.finish();
        }
        self.transmit_ethernet(MacAddr::BROADCAST, EtherType::Arp, &packet)?;
        self.arp_requests = self.arp_requests.saturating_add(1);
        Ok(())
    }

    fn send_arp_reply(&mut self, target: Ipv4Addr, target_mac: [u8; 6]) -> Result<(), Errno> {
        let config = self.ipv4.ok_or(Errno::ENETUNREACH)?;
        let caps = self.device.capabilities();
        let source = self
            .config
            .hardware_addr
            .or(caps.hardware_addr)
            .ok_or(Errno::ENETUNREACH)?;
        let mut packet = [0u8; 28];
        {
            let builder = ArpBuilder::new(
                &mut packet,
                2,
                MacAddr::new(source),
                to_wire(config.addr),
                MacAddr::new(target_mac),
                to_wire(target),
            )
            .map_err(|_| Errno::EINVAL)?;
            builder.finish();
        }
        self.transmit_ethernet(MacAddr::new(target_mac), EtherType::Arp, &packet)
    }

    pub(crate) fn process_ip_timers(&mut self, now: Instant) {
        let _ = self.reassembly.expire(now);
        let due = self.neighbors.due_probes(now);
        for address in due {
            let retries = match self.neighbors.get(address) {
                Some(entry) if entry.mac.is_none() => entry.retries,
                _ => continue,
            };
            if retries >= ARP_MAX_RETRIES {
                if let Some(packet) = self.neighbors.fail(address) {
                    self.drops = self.drops.saturating_add(1);
                    self.ip_errors
                        .push((packet.destination, Errno::EHOSTUNREACH));
                }
                continue;
            }
            let backoff = 1000u64.saturating_mul(1u64 << u32::from(retries.min(10)));
            let deadline = Instant::from_millis(
                now.as_millis()
                    .saturating_add(i64::try_from(backoff).unwrap_or(i64::MAX)),
            );
            if self.neighbors.refresh_probe(address, deadline) {
                let _ = self.send_arp_request(address);
            }
        }
        let _ = self.neighbors.expire(now);
    }

    /// Inject a raw Ethernet frame into the input path (host tests and the simulator).
    #[cfg(any(test, feature = "testutil"))]
    pub fn test_inject_frame(&mut self, frame: &[u8], now: Instant) {
        self.now = now;
        self.handle_frame(frame, now);
    }

    /// Send an IPv4 datagram through the configured route (host tests).
    #[cfg(any(test, feature = "testutil"))]
    pub fn test_send_ipv4(
        &mut self,
        payload: &[u8],
        destination: Ipv4Addr,
        protocol: u8,
    ) -> Result<(), Errno> {
        self.send_ipv4(payload, destination, protocol)
    }

    /// Take datagrams delivered to the (not yet implemented) transport layer.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_take_delivered(&mut self) -> Vec<(u8, Vec<u8>)> {
        core::mem::take(&mut self.delivered)
    }

    /// Take IP-layer errors.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_take_ip_errors(&mut self) -> Vec<(Ipv4Addr, Errno)> {
        core::mem::take(&mut self.ip_errors)
    }

    /// Number of partial reassemblies.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_reassembly_entries(&self) -> usize {
        self.reassembly.len()
    }

    /// Bytes currently held by reassembly.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_reassembly_used(&self) -> usize {
        self.reassembly.used_bytes()
    }

    /// Reassembly datagrams dropped.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_reassembly_dropped(&self) -> usize {
        self.reassembly.dropped()
    }

    /// Reassembly datagrams completed.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_reassembly_completed(&self) -> usize {
        self.reassembly.completed()
    }

    /// ARP requests transmitted.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_arp_requests(&self) -> usize {
        self.arp_requests
    }

    /// Insert a resolved neighbour directly (test setup).
    #[cfg(any(test, feature = "testutil"))]
    pub fn test_insert_neighbor(
        &mut self,
        address: Ipv4Addr,
        mac: [u8; 6],
        now: Instant,
        lifetime_ms: u64,
    ) {
        self.neighbors
            .insert_resolved(address, mac, now, lifetime_ms);
    }
}
