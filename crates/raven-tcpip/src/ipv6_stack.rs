// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! IPv6 addressing, neighbour discovery, SLAAC, ICMPv6 and the v6 data path.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use core::fmt;

use raven_abi::errno::Errno;
use raven_tcpip_wire::checksum::ChecksumContext;
use raven_tcpip_wire::ethernet::{EtherType, MacAddr};
use raven_tcpip_wire::icmpv6::{Icmpv6Builder, Icmpv6Packet};
use raven_tcpip_wire::ipv6::{
    ExtensionKind, Ipv6Addr as WireIpv6Addr, Ipv6Builder, Ipv6Header, Ipv6Packet,
};
use raven_tcpip_wire::udp::UdpPacket;

use crate::device::{Device, Medium};
use crate::socket::Kind;
use crate::{Instant, Ipv4Addr, SocketId, Stack};

const DAD_DELAY_MS: u64 = 1000;
const REACHABLE_MS: u64 = 30_000;
const DELAY_MS: u64 = 5_000;
const PROBE_MS: u64 = 5_000;
const MAX_ND_RETRIES: u8 = 3;
const MIN_IPV6_MTU: usize = 1280;
const TWO_HOURS_MS: u64 = 2 * 60 * 60 * 1000;

/// An IPv6 address.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Ipv6Addr([u8; 16]);

impl Ipv6Addr {
    /// The unspecified address `::`.
    pub const UNSPECIFIED: Self = Self([0; 16]);
    /// The loopback address `::1`.
    pub const LOOPBACK: Self = Self([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    /// The all-nodes link-local multicast address `ff02::1`.
    pub const ALL_NODES: Self = Self([0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);

    /// Construct from sixteen octets.
    #[must_use]
    pub const fn new(octets: [u8; 16]) -> Self {
        Self(octets)
    }

    /// The sixteen octets in wire order.
    #[must_use]
    pub const fn octets(self) -> [u8; 16] {
        self.0
    }

    /// Construct from a slice if it is exactly sixteen octets long.
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        match bytes {
            &[a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p] => {
                Some(Self([a, b, c, d, e, f, g, h, i, j, k, l, m, n, o, p]))
            }
            _ => None,
        }
    }

    /// Whether this is the unspecified address.
    #[must_use]
    pub const fn is_unspecified(self) -> bool {
        matches!(self.0, [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0])
    }

    /// Whether this is a link-local unicast address (`fe80::/10`).
    #[must_use]
    pub const fn is_link_local(self) -> bool {
        self.0[0] == 0xfe && self.0[1] & 0xc0 == 0x80
    }

    /// Whether this is a multicast address (`ff00::/8`).
    #[must_use]
    pub const fn is_multicast(self) -> bool {
        self.0[0] == 0xff
    }

    /// The solicited-node multicast address for this unicast address.
    #[must_use]
    pub const fn solicited_node(self) -> Self {
        let mut octets = [0xff, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        octets[11] = 0x01;
        octets[12] = 0xff;
        octets[13] = self.0[13];
        octets[14] = self.0[14];
        octets[15] = self.0[15];
        Self(octets)
    }

    /// Convert an IPv4-mapped address `::ffff:a.b.c.d`.
    #[must_use]
    pub const fn to_ipv4_mapped(self) -> Option<Ipv4Addr> {
        if self.0[0] == 0
            && self.0[1] == 0
            && self.0[2] == 0
            && self.0[3] == 0
            && self.0[4] == 0
            && self.0[5] == 0
            && self.0[6] == 0
            && self.0[7] == 0
            && self.0[8] == 0
            && self.0[9] == 0
            && self.0[10] == 0xff
            && self.0[11] == 0xff
        {
            Some(Ipv4Addr::new([
                self.0[12], self.0[13], self.0[14], self.0[15],
            ]))
        } else {
            None
        }
    }

    /// Build an IPv4-mapped address from an IPv4 address.
    #[must_use]
    pub const fn from_ipv4_mapped(address: Ipv4Addr) -> Self {
        let octets = address.octets();
        Self([
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, octets[0], octets[1], octets[2], octets[3],
        ])
    }

    /// The link-local address derived from an Ethernet MAC (RFC 4291 modified EUI-64).
    #[must_use]
    pub const fn link_local_from_mac(mac: [u8; 6]) -> Self {
        let first = mac[0] ^ 0x02;
        Self([
            0xfe, 0x80, 0, 0, 0, 0, 0, 0, first, mac[1], mac[2], 0xff, 0xfe, mac[3], mac[4], mac[5],
        ])
    }

    /// Whether `self` and `other` share `prefix_len` bits.
    #[must_use]
    pub fn same_prefix(self, other: Self, prefix_len: u8) -> bool {
        let bits = usize::from(prefix_len.min(128));
        let bytes = bits / 8;
        let remainder = bits % 8;
        if self.0[..bytes] != other.0[..bytes] {
            return false;
        }
        if remainder == 0 {
            return true;
        }
        let mask = 0xffu8 << (8 - remainder);
        self.0[bytes] & mask == other.0[bytes] & mask
    }
}

impl From<[u8; 16]> for Ipv6Addr {
    fn from(octets: [u8; 16]) -> Self {
        Self(octets)
    }
}

impl fmt::Debug for Ipv6Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl fmt::Display for Ipv6Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let o = self.0;
        write!(
            f,
            "{:04x}:{:04x}:{:04x}:{:04x}:{:04x}:{:04x}:{:04x}:{:04x}",
            u16::from_be_bytes([o[0], o[1]]),
            u16::from_be_bytes([o[2], o[3]]),
            u16::from_be_bytes([o[4], o[5]]),
            u16::from_be_bytes([o[6], o[7]]),
            u16::from_be_bytes([o[8], o[9]]),
            u16::from_be_bytes([o[10], o[11]]),
            u16::from_be_bytes([o[12], o[13]]),
            u16::from_be_bytes([o[14], o[15]]),
        )
    }
}

/// An IPv6 prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ipv6Cidr {
    /// Prefix address.
    pub addr: Ipv6Addr,
    /// Prefix length, 0..=128.
    pub prefix_len: u8,
}

/// The IPv6 configuration installed on a stack.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ipv6Config {
    /// Configured addresses.
    pub addrs: Vec<Ipv6Cidr>,
    /// Optional default gateway.
    pub gateway: Option<Ipv6Addr>,
}

/// A version-agnostic address at the socket boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IpAddr {
    /// IPv4.
    V4(Ipv4Addr),
    /// IPv6.
    V6(Ipv6Addr),
}

impl From<Ipv4Addr> for IpAddr {
    fn from(address: Ipv4Addr) -> Self {
        Self::V4(address)
    }
}

impl From<Ipv6Addr> for IpAddr {
    fn from(address: Ipv6Addr) -> Self {
        Self::V6(address)
    }
}

impl fmt::Display for IpAddr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::V4(address) => fmt::Display::fmt(address, f),
            Self::V6(address) => fmt::Display::fmt(address, f),
        }
    }
}

/// The lifecycle state of an IPv6 address.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ipv6AddressState {
    /// Duplicate-address detection is still running; the address is unusable.
    Tentative,
    /// Usable for new communication.
    Usable,
    /// Valid but no longer preferred.
    Deprecated,
    /// A duplicate was detected.
    Conflict,
}

pub(crate) struct V6Address {
    pub(crate) state: Ipv6AddressState,
    #[allow(dead_code)] // Read by the testutil lifetime hooks.
    pub(crate) preferred_until: Instant,
    pub(crate) valid_until: Instant,
}

/// The neighbour-discovery state of an IPv6 neighbour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Neighbor6State {
    /// Address resolution is in progress.
    Incomplete,
    /// Forwarding confirmed recently.
    Reachable,
    /// No recent confirmation.
    Stale,
    /// Waiting before probing.
    Delay,
    /// Probing with a neighbour solicitation.
    Probe,
}

/// One IPv6 neighbour entry.
#[derive(Clone, Debug)]
pub struct Neighbor6Entry {
    /// Resolved link-layer address.
    pub mac: Option<[u8; 6]>,
    /// Current state.
    pub state: Neighbor6State,
    /// Deadline for the current state.
    pub deadline: Instant,
    /// Retry count.
    pub retries: u8,
    pub(crate) queued: Option<Queued6>,
}

#[derive(Clone, Debug)]
pub(crate) struct Queued6 {
    pub(crate) source: Ipv6Addr,
    pub(crate) destination: Ipv6Addr,
    pub(crate) protocol: u8,
    pub(crate) payload: Vec<u8>,
}

/// A bounded IPv6 neighbour cache.
#[derive(Clone, Debug)]
pub struct Neighbor6Cache {
    entries: BTreeMap<Ipv6Addr, Neighbor6Entry>,
    capacity: usize,
}

impl Neighbor6Cache {
    /// Create a cache with `capacity` entries.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            capacity,
        }
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the cache is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Look up an entry.
    #[must_use]
    pub fn get(&self, address: Ipv6Addr) -> Option<&Neighbor6Entry> {
        self.entries.get(&address)
    }

    /// Iterate over entries.
    pub fn iter(&self) -> impl Iterator<Item = (Ipv6Addr, &Neighbor6Entry)> {
        self.entries
            .iter()
            .map(|(address, entry)| (*address, entry))
    }

    fn make_room(&mut self, address: Ipv6Addr) {
        if self.entries.contains_key(&address) || self.entries.len() < self.capacity {
            return;
        }
        if let Some(oldest) = self.entries.keys().next().copied() {
            self.entries.remove(&oldest);
        }
    }

    fn insert(&mut self, address: Ipv6Addr, entry: Neighbor6Entry) {
        self.make_room(address);
        self.entries.insert(address, entry);
    }
}

impl<D: Device> Stack<D> {
    /// Install IPv6 addresses and an optional default gateway.
    pub fn set_ipv6(&mut self, config: Ipv6Config) -> Result<(), Errno> {
        for cidr in &config.addrs {
            if cidr.prefix_len > 128 {
                return Err(Errno::EINVAL);
            }
        }
        self.ipv6 = config;
        self.ipv6_gateway = self.ipv6.gateway;
        let forever = Instant::from_millis(i64::MAX);
        for cidr in &self.ipv6.addrs {
            self.ipv6_states.insert(
                cidr.addr,
                V6Address {
                    state: Ipv6AddressState::Usable,
                    preferred_until: forever,
                    valid_until: forever,
                },
            );
        }
        Ok(())
    }

    /// Enable SLAAC: install a tentative link-local address and accept router advertisements.
    pub fn enable_slaac(&mut self) -> Result<(), Errno> {
        self.slaac_enabled = true;
        let caps = self.device.capabilities();
        let mac = self
            .config
            .hardware_addr
            .or(caps.hardware_addr)
            .ok_or(Errno::EADDRNOTAVAIL)?;
        let link_local = Ipv6Addr::link_local_from_mac(mac);
        if !self.ipv6_states.contains_key(&link_local) {
            self.ipv6.addrs.push(Ipv6Cidr {
                addr: link_local,
                prefix_len: 64,
            });
            let deadline = Instant::from_millis(
                self.now
                    .as_millis()
                    .saturating_add(i64::try_from(DAD_DELAY_MS).unwrap_or(i64::MAX)),
            );
            self.ipv6_dad.push((link_local, deadline));
            self.ipv6_states.insert(
                link_local,
                V6Address {
                    state: Ipv6AddressState::Tentative,
                    preferred_until: Instant::from_millis(i64::MAX),
                    valid_until: Instant::from_millis(i64::MAX),
                },
            );
            let _ = self.send_neighbor_solicitation(link_local, true);
        }
        Ok(())
    }

    /// The IPv6 configuration.
    #[must_use]
    pub const fn ipv6(&self) -> &Ipv6Config {
        &self.ipv6
    }

    /// Accept or reject IPv4-mapped IPv6 traffic. Default is reject.
    pub fn set_v4_mapped(&mut self, enabled: bool) {
        self.v4_mapped_enabled = enabled;
    }

    /// The IPv6 neighbour cache.
    #[must_use]
    pub const fn neighbors6(&self) -> &Neighbor6Cache {
        &self.ipv6_neighbors
    }

    pub(crate) fn usable_ipv6(&self) -> Option<Ipv6Addr> {
        for cidr in &self.ipv6.addrs {
            if self
                .ipv6_states
                .get(&cidr.addr)
                .is_some_and(|address| address.state == Ipv6AddressState::Usable)
            {
                return Some(cidr.addr);
            }
        }
        None
    }

    fn has_usable_ipv6(&self, address: Ipv6Addr) -> bool {
        self.ipv6_states
            .get(&address)
            .is_some_and(|state| state.state == Ipv6AddressState::Usable)
    }

    fn is_for_us(&self, destination: Ipv6Addr) -> bool {
        if self.ipv6_states.contains_key(&destination) {
            return true;
        }
        if destination == Ipv6Addr::ALL_NODES {
            return true;
        }
        self.ipv6
            .addrs
            .iter()
            .any(|cidr| destination.is_multicast() && destination == cidr.addr.solicited_node())
    }

    pub(crate) fn process_ipv6_timers(&mut self, now: Instant) {
        let mut completed = Vec::new();
        self.ipv6_dad.retain(|(address, deadline)| {
            if *deadline <= now {
                completed.push(*address);
                false
            } else {
                true
            }
        });
        for address in completed {
            if let Some(state) = self.ipv6_states.get_mut(&address)
                && state.state == Ipv6AddressState::Tentative
            {
                state.state = Ipv6AddressState::Usable;
            }
        }

        let addresses: Vec<Ipv6Addr> = self.ipv6_neighbors.entries.keys().copied().collect();
        for address in addresses {
            let state = match self.ipv6_neighbors.get(address) {
                Some(entry) => (entry.state, entry.deadline),
                None => continue,
            };
            match state {
                (Neighbor6State::Reachable, deadline) if deadline <= now => {
                    if let Some(entry) = self.ipv6_neighbors.entries.get_mut(&address) {
                        entry.state = Neighbor6State::Stale;
                    }
                }
                (Neighbor6State::Delay, deadline) if deadline <= now => {
                    if let Some(entry) = self.ipv6_neighbors.entries.get_mut(&address) {
                        entry.state = Neighbor6State::Probe;
                        entry.deadline = Instant::from_millis(
                            now.as_millis()
                                .saturating_add(i64::try_from(PROBE_MS).unwrap_or(i64::MAX)),
                        );
                    }
                    let _ = self.send_neighbor_solicitation(address, false);
                }
                (Neighbor6State::Probe, deadline) if deadline <= now => {
                    let retries = self
                        .ipv6_neighbors
                        .get(address)
                        .map_or(0, |entry| entry.retries);
                    if retries >= MAX_ND_RETRIES {
                        if let Some(entry) = self.ipv6_neighbors.entries.remove(&address)
                            && let Some(packet) = entry.queued
                        {
                            self.ip_errors
                                .push((Ipv4Addr::UNSPECIFIED, Errno::EHOSTUNREACH));
                            let _ = packet;
                        }
                        self.drops = self.drops.saturating_add(1);
                    } else if let Some(entry) = self.ipv6_neighbors.entries.get_mut(&address) {
                        entry.retries = entry.retries.saturating_add(1);
                        entry.deadline = Instant::from_millis(
                            now.as_millis()
                                .saturating_add(i64::try_from(PROBE_MS).unwrap_or(i64::MAX)),
                        );
                        let _ = self.send_neighbor_solicitation(address, false);
                    }
                }
                _ => {}
            }
        }
    }

    pub(crate) fn handle_ipv6(&mut self, payload: &[u8], now: Instant) {
        let Ok(packet) = Ipv6Packet::parse(payload) else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        let destination = from_wire(packet.dst());
        let source = from_wire(packet.src());

        if let Some(mapped) = destination.to_ipv4_mapped() {
            if !self.v4_mapped_enabled {
                self.drops = self.drops.saturating_add(1);
                return;
            }
            let (next, upper) = packet.upper_layer();
            if next == 17 {
                self.handle_udp_mapped(source, mapped, upper);
            }
            return;
        }

        if !self.is_for_us(destination) {
            self.drops = self.drops.saturating_add(1);
            return;
        }

        for extension in packet.extensions() {
            if matches!(
                extension.kind,
                ExtensionKind::HopByHop | ExtensionKind::Destination
            ) && let Some(issue) = check_options(extension.bytes.get(2..).unwrap_or(&[]))
            {
                match issue {
                    OptionIssue::Discard => {}
                    OptionIssue::Report(pointer) => {
                        self.send_icmpv6_parameter_problem(source, pointer, payload);
                    }
                }
                return;
            }
        }

        let (next, upper) = packet.upper_layer();
        let fragmented = packet.fragment_header().is_some();
        if next != 58 && !self.has_usable_ipv6(destination) {
            self.drops = self.drops.saturating_add(1);
            return;
        }
        match next {
            58 => self.handle_icmpv6(
                source,
                destination,
                packet.hop_limit(),
                fragmented,
                upper,
                now,
            ),
            17 => self.handle_udp6(source, destination, upper),
            59 => {}
            _ => self.send_icmpv6_unrecognized_next_header(source, payload),
        }
    }

    pub(crate) fn send_ipv6(
        &mut self,
        payload: &[u8],
        destination: Ipv6Addr,
        protocol: u8,
    ) -> Result<(), Errno> {
        if let Some(mapped) = destination.to_ipv4_mapped() {
            if !self.v4_mapped_enabled {
                return Err(Errno::EAFNOSUPPORT);
            }
            return self.send_ipv4(payload, mapped, protocol);
        }
        let source = self.usable_ipv6().ok_or(Errno::ENETUNREACH)?;
        if destination.is_multicast() {
            return Err(Errno::ENETUNREACH);
        }
        let next_hop = match self.ipv6_gateway {
            Some(gateway) if !destination.is_link_local() => gateway,
            _ => destination,
        };
        let entry = self.ipv6_neighbors.get(next_hop).cloned();
        match entry {
            Some(entry)
                if entry.mac.is_some()
                    && matches!(
                        entry.state,
                        Neighbor6State::Reachable
                            | Neighbor6State::Stale
                            | Neighbor6State::Delay
                            | Neighbor6State::Probe
                    ) =>
            {
                let mac = entry.mac.unwrap_or([0; 6]);
                if entry.state == Neighbor6State::Stale
                    && let Some(entry) = self.ipv6_neighbors.entries.get_mut(&next_hop)
                {
                    entry.state = Neighbor6State::Delay;
                    entry.deadline = Instant::from_millis(
                        self.now
                            .as_millis()
                            .saturating_add(i64::try_from(DELAY_MS).unwrap_or(i64::MAX)),
                    );
                }
                self.transmit_ipv6(mac, source, destination, protocol, payload)
            }
            Some(entry) if entry.queued.is_some() => {
                self.drops = self.drops.saturating_add(1);
                Err(Errno::EAGAIN)
            }
            Some(_) => {
                if let Some(entry) = self.ipv6_neighbors.entries.get_mut(&next_hop) {
                    entry.queued = Some(Queued6 {
                        source,
                        destination,
                        protocol,
                        payload: payload.to_vec(),
                    });
                    entry.deadline =
                        Instant::from_millis(self.now.as_millis().saturating_add(1000));
                }
                let _ = self.send_neighbor_solicitation(next_hop, false);
                Ok(())
            }
            None => {
                self.ipv6_neighbors.insert(
                    next_hop,
                    Neighbor6Entry {
                        mac: None,
                        state: Neighbor6State::Incomplete,
                        deadline: Instant::from_millis(self.now.as_millis().saturating_add(1000)),
                        retries: 0,
                        queued: Some(Queued6 {
                            source,
                            destination,
                            protocol,
                            payload: payload.to_vec(),
                        }),
                    },
                );
                let _ = self.send_neighbor_solicitation(next_hop, false);
                Ok(())
            }
        }
    }

    fn transmit_ipv6(
        &mut self,
        mac: [u8; 6],
        source: Ipv6Addr,
        destination: Ipv6Addr,
        protocol: u8,
        payload: &[u8],
    ) -> Result<(), Errno> {
        let caps = self.device.capabilities();
        let link_mtu = match caps.medium {
            Medium::Ethernet => caps.mtu.saturating_sub(14),
            Medium::Ip => caps.mtu,
        };
        if payload.len().saturating_add(40) > link_mtu {
            return Err(Errno::EINVAL);
        }
        let header = Ipv6Header {
            next_header: protocol,
            hop_limit: 64,
            src: to_wire(source),
            dst: to_wire(destination),
            ..Ipv6Header::default()
        };
        let mut packet = vec![0u8; 40 + payload.len()];
        {
            let mut builder =
                Ipv6Builder::new(&mut packet, &header, payload.len()).map_err(|_| Errno::EINVAL)?;
            builder.payload_mut().copy_from_slice(payload);
        }
        self.transmit_ethernet(MacAddr::new(mac), EtherType::Ipv6, &packet)
    }

    fn send_neighbor_solicitation(
        &mut self,
        target: Ipv6Addr,
        from_unspecified: bool,
    ) -> Result<(), Errno> {
        let source = if from_unspecified {
            Ipv6Addr::UNSPECIFIED
        } else {
            self.usable_ipv6().ok_or(Errno::ENETUNREACH)?
        };
        let destination = target.solicited_node();
        let mut body = vec![0u8; 16];
        body.copy_from_slice(&target.octets());
        let context = ChecksumContext::ipv6(to_wire(source), to_wire(destination));
        let mut message = vec![0u8; 8 + body.len()];
        {
            let mut builder = Icmpv6Builder::new(&mut message, context, 135, 0, 0, body.len())
                .map_err(|_| Errno::EINVAL)?;
            builder.payload_mut().copy_from_slice(&body);
            builder.finish();
        }
        let mac = [
            0x33,
            0x33,
            0xff,
            target.octets()[13],
            target.octets()[14],
            target.octets()[15],
        ];
        self.transmit_ipv6(mac, source, destination, 58, &message)
    }

    fn send_neighbor_advertisement(
        &mut self,
        destination: Ipv6Addr,
        target: Ipv6Addr,
        mac: [u8; 6],
    ) -> Result<(), Errno> {
        let source = target;
        let mut body = vec![0u8; 16 + 8];
        body[..16].copy_from_slice(&target.octets());
        body[16] = 2;
        body[17] = 1;
        body[18..24].copy_from_slice(&mac);
        let context = ChecksumContext::ipv6(to_wire(source), to_wire(destination));
        let mut message = vec![0u8; 8 + body.len()];
        {
            let mut builder =
                Icmpv6Builder::new(&mut message, context, 136, 0, 0x6000_0000, body.len())
                    .map_err(|_| Errno::EINVAL)?;
            builder.payload_mut().copy_from_slice(&body);
            builder.finish();
        }
        let ether = if destination.is_multicast() {
            [0x33, 0x33, 0, 0, 0, 1]
        } else {
            self.ipv6_neighbors
                .get(destination)
                .and_then(|entry| entry.mac)
                .unwrap_or([0x33, 0x33, 0, 0, 0, 1])
        };
        self.transmit_ipv6(ether, source, destination, 58, &message)
    }

    fn handle_icmpv6(
        &mut self,
        source: Ipv6Addr,
        destination: Ipv6Addr,
        hop_limit: u8,
        fragmented: bool,
        payload: &[u8],
        now: Instant,
    ) {
        let context = ChecksumContext::ipv6(to_wire(source), to_wire(destination));
        let Ok(message) = Icmpv6Packet::parse(payload, context) else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        let message_type = message.message_type();
        if (133..=137).contains(&message_type) {
            if hop_limit != 255 {
                self.nd_bad_hop_limit = self.nd_bad_hop_limit.saturating_add(1);
                return;
            }
            if fragmented {
                self.nd_fragments = self.nd_fragments.saturating_add(1);
                return;
            }
        }
        match message_type {
            128 => {
                let _ = self.send_icmpv6_message(
                    129,
                    message.code(),
                    message.rest(),
                    message.payload(),
                    source,
                );
            }
            129 => self.deliver_echo_reply6(&message),
            135 => self.handle_neighbor_solicitation(source, &message, now),
            136 => self.handle_neighbor_advertisement(&message, now),
            133 => {
                self.router_solicitations = self.router_solicitations.saturating_add(1);
            }
            134 => self.handle_router_advertisement(source, &message, now),
            1..=4 => self.handle_icmpv6_error(&message),
            _ => {}
        }
    }

    fn handle_neighbor_solicitation(
        &mut self,
        source: Ipv6Addr,
        message: &Icmpv6Packet<'_>,
        now: Instant,
    ) {
        let Some(target) = message.target() else {
            return;
        };
        let target = from_wire(target);
        if self
            .ipv6_states
            .get(&target)
            .is_some_and(|state| state.state == Ipv6AddressState::Tentative)
        {
            if source.is_unspecified() {
                if let Some(state) = self.ipv6_states.get_mut(&target) {
                    state.state = Ipv6AddressState::Conflict;
                }
                self.dad_conflicts = self.dad_conflicts.saturating_add(1);
                self.drops = self.drops.saturating_add(1);
            }
            return;
        }
        if !self.has_usable_ipv6(target) {
            return;
        }
        if !source.is_unspecified() {
            self.learn_neighbor6(source, slla_from_options(message.payload(), source), now);
            let _ = self.send_neighbor_advertisement(
                source,
                target,
                mac_from_options(message.payload(), [0; 6]),
            );
        }
    }

    fn handle_neighbor_advertisement(&mut self, message: &Icmpv6Packet<'_>, now: Instant) {
        let Some(target) = message.target() else {
            return;
        };
        let target = from_wire(target);
        let mac = mac_from_options(message.payload(), [0; 6]);
        if let Some(state) = self.ipv6_states.get_mut(&target)
            && state.state == Ipv6AddressState::Tentative
        {
            state.state = Ipv6AddressState::Conflict;
            self.dad_conflicts = self.dad_conflicts.saturating_add(1);
            return;
        }
        self.learn_neighbor6(target, mac, now);
        let queued = self
            .ipv6_neighbors
            .entries
            .get_mut(&target)
            .and_then(|entry| entry.queued.take());
        if let Some(packet) = queued {
            let _ = self.transmit_ipv6(
                mac,
                packet.source,
                packet.destination,
                packet.protocol,
                &packet.payload,
            );
        }
    }

    fn learn_neighbor6(&mut self, address: Ipv6Addr, mac: [u8; 6], now: Instant) {
        let entry = Neighbor6Entry {
            mac: Some(mac),
            state: Neighbor6State::Reachable,
            deadline: Instant::from_millis(
                now.as_millis()
                    .saturating_add(i64::try_from(REACHABLE_MS).unwrap_or(i64::MAX)),
            ),
            retries: 0,
            queued: self
                .ipv6_neighbors
                .get(address)
                .and_then(|entry| entry.queued.clone()),
        };
        self.ipv6_neighbors.insert(address, entry);
    }

    fn handle_router_advertisement(
        &mut self,
        router: Ipv6Addr,
        message: &Icmpv6Packet<'_>,
        now: Instant,
    ) {
        if !self.slaac_enabled {
            return;
        }
        let body = message.payload();
        if body.len() < 12 {
            return;
        }
        let router_lifetime = u16::from_be_bytes([body[2], body[3]]);
        if router_lifetime > 0 {
            self.ipv6_gateway = Some(router);
        }
        let mut offset = 12;
        while offset + 2 <= body.len() {
            let option_type = body[offset];
            let length = usize::from(body[offset + 1]) * 8;
            if length == 0 || offset + length > body.len() {
                break;
            }
            let option = &body[offset..offset + length];
            if option_type == 3 && length >= 32 {
                let valid = u32::from_be_bytes([option[4], option[5], option[6], option[7]]);
                let preferred = u32::from_be_bytes([option[8], option[9], option[10], option[11]]);
                let prefix = Ipv6Addr::from_bytes(&option[16..32]).unwrap_or(Ipv6Addr::UNSPECIFIED);
                if option[2] == 64 {
                    let link_local = self
                        .ipv6
                        .addrs
                        .iter()
                        .map(|cidr| cidr.addr)
                        .find(|address| address.is_link_local())
                        .unwrap_or_else(|| {
                            Ipv6Addr::link_local_from_mac(
                                self.config
                                    .hardware_addr
                                    .or(self.device.capabilities().hardware_addr)
                                    .unwrap_or([0; 6]),
                            )
                        });
                    let mut octets = prefix.octets();
                    octets[8..16].copy_from_slice(&link_local.octets()[8..16]);
                    let address = Ipv6Addr::new(octets);
                    self.install_slaac_address(address, valid, preferred, now);
                }
            }
            offset += length;
        }
    }

    fn install_slaac_address(
        &mut self,
        address: Ipv6Addr,
        valid_seconds: u32,
        preferred_seconds: u32,
        now: Instant,
    ) {
        let remaining = self
            .ipv6_states
            .get(&address)
            .map(|state| state.valid_until.saturating_since(now).as_millis())
            .unwrap_or(0);
        let mut valid_ms = u64::from(valid_seconds).saturating_mul(1000);
        if self.ipv6_states.contains_key(&address)
            && valid_ms > TWO_HOURS_MS
            && remaining <= TWO_HOURS_MS
        {
            valid_ms = TWO_HOURS_MS;
        }
        let preferred_ms = u64::from(preferred_seconds)
            .saturating_mul(1000)
            .min(valid_ms);
        let valid_until = Instant::from_millis(
            now.as_millis()
                .saturating_add(i64::try_from(valid_ms).unwrap_or(i64::MAX)),
        );
        let preferred_until = Instant::from_millis(
            now.as_millis()
                .saturating_add(i64::try_from(preferred_ms).unwrap_or(i64::MAX)),
        );
        let state = if valid_ms == 0 || preferred_ms == 0 {
            Ipv6AddressState::Deprecated
        } else {
            Ipv6AddressState::Usable
        };
        if !self.ipv6.addrs.iter().any(|cidr| cidr.addr == address) {
            self.ipv6.addrs.push(Ipv6Cidr {
                addr: address,
                prefix_len: 64,
            });
        }
        self.ipv6_states.insert(
            address,
            V6Address {
                state,
                preferred_until,
                valid_until,
            },
        );
    }

    fn deliver_echo_reply6(&mut self, message: &Icmpv6Packet<'_>) {
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

    fn handle_icmpv6_error(&mut self, message: &Icmpv6Packet<'_>) {
        let quoted = message.payload();
        let Ok(inner) = Ipv6Packet::parse(quoted) else {
            return;
        };
        let (next, upper) = inner.upper_layer();
        if next != 17 || upper.len() < 8 {
            return;
        }
        let source_port = u16::from_be_bytes([upper[0], upper[1]]);
        let destination_port = u16::from_be_bytes([upper[2], upper[3]]);
        let _ = source_port;
        let quoted_destination = from_wire(inner.dst());
        let error = match (message.message_type(), message.code()) {
            (1, 0) => Errno::ENETUNREACH,
            (1, 1) => Errno::EHOSTUNREACH,
            (1, 3) => Errno::EADDRNOTAVAIL,
            (1, 4) => Errno::ECONNREFUSED,
            (2, _) => {
                let mtu = usize::try_from(message.rest()).unwrap_or(MIN_IPV6_MTU);
                self.pmtu6.insert(quoted_destination, mtu.max(MIN_IPV6_MTU));
                Errno::EINVAL
            }
            (3, _) => Errno::ETIMEDOUT,
            (4, _) => Errno::EPROTO,
            _ => Errno::EPROTO,
        };
        let socket = self.sockets.records.iter().find_map(|(raw_id, record)| {
            if record.closed {
                return None;
            }
            match &record.kind {
                Kind::Udp { local_port, .. }
                    if *local_port == destination_port || *local_port == source_port =>
                {
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

    fn send_icmpv6_message(
        &mut self,
        message_type: u8,
        code: u8,
        rest: u32,
        payload: &[u8],
        destination: Ipv6Addr,
    ) -> Result<(), Errno> {
        let source = self.usable_ipv6().ok_or(Errno::ENETUNREACH)?;
        let context = ChecksumContext::ipv6(to_wire(source), to_wire(destination));
        let mut message = vec![0u8; 8 + payload.len()];
        {
            let mut builder = Icmpv6Builder::new(
                &mut message,
                context,
                message_type,
                code,
                rest,
                payload.len(),
            )
            .map_err(|_| Errno::EINVAL)?;
            builder.payload_mut().copy_from_slice(payload);
            builder.finish();
        }
        self.send_ipv6(&message, destination, 58)
    }

    fn send_icmpv6_parameter_problem(
        &mut self,
        destination: Ipv6Addr,
        pointer: usize,
        quoted: &[u8],
    ) {
        let quote = &quoted[..quoted.len().min(1280)];
        if self
            .send_icmpv6_message(
                4,
                2,
                u32::try_from(pointer).unwrap_or(0),
                quote,
                destination,
            )
            .is_ok()
        {
            self.icmpv6_errors_sent = self.icmpv6_errors_sent.saturating_add(1);
        }
    }

    fn send_icmpv6_unrecognized_next_header(&mut self, destination: Ipv6Addr, quoted: &[u8]) {
        let quote = &quoted[..quoted.len().min(1280)];
        if self
            .send_icmpv6_message(4, 1, 6, quote, destination)
            .is_ok()
        {
            self.icmpv6_errors_sent = self.icmpv6_errors_sent.saturating_add(1);
        }
    }

    fn send_port_unreachable6(&mut self, source: Ipv6Addr, destination: Ipv6Addr, original: &[u8]) {
        if destination.is_multicast() || !self.allow_icmp_error(self.now) {
            return;
        }
        // Build the quoted IPv6 header plus the first eight transport octets.
        let quote_len = original.len().min(8);
        let mut packet = vec![0u8; 40 + quote_len];
        let header = Ipv6Header {
            next_header: 17,
            hop_limit: 64,
            src: to_wire(destination),
            dst: to_wire(source),
            ..Ipv6Header::default()
        };
        if let Ok(mut builder) = Ipv6Builder::new(&mut packet, &header, quote_len) {
            builder
                .payload_mut()
                .copy_from_slice(&original[..quote_len]);
        }
        if self.send_icmpv6_message(1, 4, 0, &packet, source).is_ok() {
            self.icmpv6_errors_sent = self.icmpv6_errors_sent.saturating_add(1);
        }
    }

    fn handle_udp_mapped(&mut self, source: Ipv6Addr, destination: Ipv4Addr, payload: &[u8]) {
        let source = source.to_ipv4_mapped().unwrap_or(Ipv4Addr::UNSPECIFIED);
        self.handle_udp(source, destination, payload);
    }

    fn handle_udp6(&mut self, source: Ipv6Addr, destination: Ipv6Addr, payload: &[u8]) {
        let context = ChecksumContext::ipv6(to_wire(source), to_wire(destination));
        let Ok(datagram) = UdpPacket::parse(payload, context) else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        let destination_port = datagram.dst_port();
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
        let Some(socket) = socket else {
            self.drops = self.drops.saturating_add(1);
            if !destination.is_multicast() {
                self.send_port_unreachable6(source, destination, datagram.as_bytes());
            }
            return;
        };
        let Ok(record) = self.sockets.record_mut(socket) else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        if datagram.payload_len() > record.rx_cap {
            self.drops = self.drops.saturating_add(1);
            return;
        }
        let Kind::Udp { datagrams, .. } = &mut record.kind else {
            self.drops = self.drops.saturating_add(1);
            return;
        };
        datagrams.push_back(crate::socket::Datagram {
            data: datagram.payload().to_vec(),
            src: (crate::ipv6_stack::IpAddr::V6(source), datagram.src_port()),
        });
    }

    /// The state of an IPv6 address (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_ipv6_state(&self, address: Ipv6Addr) -> Option<Ipv6AddressState> {
        self.ipv6_states.get(&address).map(|state| state.state)
    }

    /// The valid-until instant of an IPv6 address (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_ipv6_valid_until(&self, address: Ipv6Addr) -> Option<Instant> {
        self.ipv6_states
            .get(&address)
            .map(|state| state.valid_until)
    }

    /// The preferred-until instant of an IPv6 address (host tests).
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_ipv6_preferred_until(&self, address: Ipv6Addr) -> Option<Instant> {
        self.ipv6_states
            .get(&address)
            .map(|state| state.preferred_until)
    }

    /// DAD conflicts observed.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_dad_conflicts(&self) -> usize {
        self.dad_conflicts
    }

    /// ND messages discarded for a non-255 hop limit.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_nd_bad_hop_limit(&self) -> usize {
        self.nd_bad_hop_limit
    }

    /// ND messages discarded because they were fragmented.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_nd_fragments(&self) -> usize {
        self.nd_fragments
    }

    /// The recorded path MTU for an IPv6 destination.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_ipv6_pmtu(&self, destination: Ipv6Addr) -> Option<usize> {
        self.pmtu6.get(&destination).copied()
    }

    /// The neighbour state for an IPv6 address.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_neighbor6_state(&self, address: Ipv6Addr) -> Option<Neighbor6State> {
        self.ipv6_neighbors.get(address).map(|entry| entry.state)
    }

    /// Insert a resolved IPv6 neighbour.
    #[cfg(any(test, feature = "testutil"))]
    pub fn test_insert_neighbor6(&mut self, address: Ipv6Addr, mac: [u8; 6], now: Instant) {
        self.ipv6_neighbors.insert(
            address,
            Neighbor6Entry {
                mac: Some(mac),
                state: Neighbor6State::Reachable,
                deadline: Instant::from_millis(
                    now.as_millis()
                        .saturating_add(i64::try_from(REACHABLE_MS).unwrap_or(i64::MAX)),
                ),
                retries: 0,
                queued: None,
            },
        );
    }

    /// Send an IPv6 datagram through the configured path (host tests).
    #[cfg(any(test, feature = "testutil"))]
    pub fn test_send_ipv6(
        &mut self,
        payload: &[u8],
        destination: Ipv6Addr,
        protocol: u8,
    ) -> Result<(), Errno> {
        self.send_ipv6(payload, destination, protocol)
    }

    /// Send an ICMPv6 echo request (host tests).
    #[cfg(any(test, feature = "testutil"))]
    pub fn test_icmpv6_echo_request(
        &mut self,
        to: Ipv6Addr,
        id: u16,
        seq: u16,
        payload: &[u8],
    ) -> Result<(), Errno> {
        self.send_icmpv6_message(128, 0, u32::from(id) << 16 | u32::from(seq), payload, to)
    }

    /// Outbound ICMPv6 errors sent.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_icmpv6_errors_sent(&self) -> usize {
        self.icmpv6_errors_sent
    }

    /// Router solicitations observed.
    #[cfg(any(test, feature = "testutil"))]
    #[must_use]
    pub fn test_router_solicitations(&self) -> usize {
        self.router_solicitations
    }
}

enum OptionIssue {
    Discard,
    Report(usize),
}

fn check_options(bytes: &[u8]) -> Option<OptionIssue> {
    let mut offset = 0;
    while offset < bytes.len() {
        let option_type = bytes[offset];
        if option_type == 0 {
            offset += 1;
            continue;
        }
        if offset + 1 >= bytes.len() {
            return Some(OptionIssue::Discard);
        }
        let length = usize::from(bytes[offset + 1]);
        if length == 0 || offset + length > bytes.len() {
            return Some(OptionIssue::Discard);
        }
        if option_type != 1 {
            match option_type & 0xc0 {
                0x40 => return Some(OptionIssue::Discard),
                0x80 | 0xc0 => return Some(OptionIssue::Report(offset)),
                _ => {}
            }
        }
        offset += length;
    }
    None
}

fn from_wire(address: WireIpv6Addr) -> Ipv6Addr {
    Ipv6Addr::new(address.octets())
}

pub(crate) fn to_wire(address: Ipv6Addr) -> WireIpv6Addr {
    WireIpv6Addr::new(address.octets())
}

fn mac_from_options(body: &[u8], fallback: [u8; 6]) -> [u8; 6] {
    let mut offset = 16;
    while offset + 2 <= body.len() {
        let option_type = body[offset];
        let length = usize::from(body[offset + 1]) * 8;
        if length == 0 || offset + length > body.len() {
            break;
        }
        if option_type == 2 && length >= 8 {
            return [
                body[offset + 2],
                body[offset + 3],
                body[offset + 4],
                body[offset + 5],
                body[offset + 6],
                body[offset + 7],
            ];
        }
        offset += length;
    }
    fallback
}

fn slla_from_options(body: &[u8], _source: Ipv6Addr) -> [u8; 6] {
    mac_from_options(body, [0; 6])
}
