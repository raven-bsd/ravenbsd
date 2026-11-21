// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded stateful connection table with pf timeouts.
//!
//! Donor: `pf_state_key_setup()` (`sys/netpfil/pf/pf.c:2059`) builds the two state keys
//! (`sk` on the wire, `nk` in the stack); `pf_find_state()` (`pf.c:2233`) looks a packet up
//! against both; `pf_create_state()` (`pf.c:6631`) fails closed at `max_states` with
//! `PFRES_MAXSTATES` instead of evicting a live connection; `pf_state_expires()` (`pf.c:2964`)
//! selects the timeout. The table is a flat vector because the 0.1.0 bound
//! ([`MAX_STATES`]) is small and deterministic.

use alloc::string::String;
use alloc::vec::Vec;

use crate::address::IpAddr;
use crate::ast::{Direction, Packet, Protocol, tcp_flag};
use crate::nat::Translation;

/// Largest number of tracked connections.
pub const MAX_STATES: usize = 4096;

/// pf-style connection timeouts in seconds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Timeouts {
    /// First TCP packet.
    pub tcp_first: u64,
    /// TCP opening (SYN+ACK, no final ACK).
    pub tcp_opening: u64,
    /// Established TCP connection.
    pub tcp_established: u64,
    /// Half-closed TCP connection.
    pub tcp_closing: u64,
    /// Both FINs seen.
    pub tcp_fin_wait: u64,
    /// RST seen.
    pub tcp_closed: u64,
    /// First UDP packet.
    pub udp_first: u64,
    /// Unidirectional UDP flow.
    pub udp_single: u64,
    /// Bidirectional UDP flow.
    pub udp_multiple: u64,
    /// First ICMP packet.
    pub icmp_first: u64,
    /// ICMP error reply.
    pub icmp_error: u64,
    /// First other-protocol packet.
    pub other_first: u64,
    /// Unidirectional other-protocol flow.
    pub other_single: u64,
    /// Bidirectional other-protocol flow.
    pub other_multiple: u64,
    /// IP fragment lifetime (`PFTM_FRAG`).
    pub fragment: u64,
}

impl Default for Timeouts {
    fn default() -> Self {
        Self {
            tcp_first: 120,
            tcp_opening: 30,
            tcp_established: 24 * 60 * 60,
            tcp_closing: 15 * 60,
            tcp_fin_wait: 45,
            tcp_closed: 90,
            udp_first: 60,
            udp_single: 30,
            udp_multiple: 60,
            icmp_first: 20,
            icmp_error: 10,
            other_first: 60,
            other_single: 30,
            other_multiple: 60,
            fragment: 60,
        }
    }
}

/// TCP connection state as tracked by the filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TcpState {
    /// Only the first packet has been seen.
    First,
    /// SYN+ACK seen; the handshake is not complete.
    Opening,
    /// ACK seen; the connection is established.
    Established,
    /// A FIN was seen in one direction.
    FinWait,
    /// Both directions sent FIN.
    Closing,
    /// RST seen.
    Closed,
}

/// The fields that identify one side of a connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateKey {
    /// Transport protocol.
    pub protocol: Protocol,
    /// Source address of the first packet.
    pub source: IpAddr,
    /// Source port (zero for ICMP).
    pub source_port: u16,
    /// Destination address of the first packet.
    pub destination: IpAddr,
    /// Destination port (zero for ICMP).
    pub destination_port: u16,
    /// Bound interface, or `None` for a floating state.
    pub interface: Option<String>,
    /// Direction of the first packet.
    pub direction: Direction,
}

impl StateKey {
    /// Build a key from a packet. `interface` is `Some` when the rule names an interface.
    #[must_use]
    pub fn from_packet(packet: &Packet<'_>, interface: Option<&str>) -> Self {
        Self {
            protocol: packet.protocol,
            source: packet.source,
            source_port: packet.source_port,
            destination: packet.destination,
            destination_port: packet.destination_port,
            interface: interface.map(String::from),
            direction: packet.direction,
        }
    }

    /// Build the wire-side key of a translated packet (donor `nk`).
    #[must_use]
    pub fn translated(
        packet: &Packet<'_>,
        interface: Option<&str>,
        translation: Translation,
    ) -> Self {
        Self {
            protocol: packet.protocol,
            source: translation.source,
            source_port: translation.source_port,
            destination: translation.destination,
            destination_port: translation.destination_port,
            interface: interface.map(String::from),
            direction: packet.direction,
        }
    }

    /// The orientation of `packet` relative to this key.
    #[must_use]
    pub fn orientation(&self, packet: &Packet<'_>) -> Option<Orientation> {
        if self.protocol != packet.protocol {
            return None;
        }
        if let Some(interface) = &self.interface
            && interface != packet.interface
        {
            return None;
        }
        if self.source == packet.source
            && self.source_port == packet.source_port
            && self.destination == packet.destination
            && self.destination_port == packet.destination_port
            && self.direction == packet.direction
        {
            return Some(Orientation::Forward);
        }
        if self.source == packet.destination
            && self.source_port == packet.destination_port
            && self.destination == packet.source
            && self.destination_port == packet.source_port
            && self.direction != packet.direction
        {
            return Some(Orientation::Reverse);
        }
        None
    }
}

/// Which side of a connection a packet belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    /// Same direction and endpoints as the creating packet.
    Forward,
    /// Return traffic: endpoints swapped and direction opposite.
    Reverse,
}

/// One tracked connection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateEntry {
    /// Connection key of the packet as evaluated (donor `sk`).
    pub key: StateKey,
    /// Wire-side key when the connection was translated (donor `nk`).
    pub peer: Option<StateKey>,
    /// Index of the rule that created the state.
    pub rule: usize,
    /// Translation applied to the creating packet.
    pub translation: Option<Translation>,
    /// Creation time in seconds.
    pub created: u64,
    /// Last packet time in seconds.
    pub last_seen: u64,
    /// Expiry deadline in seconds.
    pub expires: u64,
    /// Packets in the forward direction.
    pub forward_packets: u64,
    /// Bytes in the forward direction.
    pub forward_bytes: u64,
    /// Packets in the reverse direction.
    pub reverse_packets: u64,
    /// Bytes in the reverse direction.
    pub reverse_bytes: u64,
    /// TCP state.
    pub tcp: TcpState,
    /// A FIN was seen in the forward direction.
    pub fin_forward: bool,
    /// A FIN was seen in the reverse direction.
    pub fin_reverse: bool,
    /// A reverse packet was seen.
    pub bidirectional: bool,
}

/// The result of a state lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StateHit {
    /// The rule that created the state.
    pub rule: usize,
    /// The packet orientation.
    pub orientation: Orientation,
    /// Current TCP state (for non-TCP flows, `First`).
    pub tcp: TcpState,
    /// Translation the packet needs (forward or reversed), when the state is translated.
    pub translation: Option<Translation>,
}

/// Whether a state entry was created, already existed or was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateDisposition {
    /// No entry existed and one was created.
    Created,
    /// An entry already existed and was refreshed.
    Existing,
    /// The table is at its limit (donor `PFRES_MAXSTATES`); the packet must be dropped.
    Rejected,
}

/// The bounded connection table.
#[derive(Clone, Debug)]
pub struct StateTable {
    entries: Vec<StateEntry>,
    limit: usize,
    timeouts: Timeouts,
}

impl StateTable {
    /// Build a table with the pf default limit and timeouts.
    #[must_use]
    pub fn new(timeouts: Timeouts) -> Self {
        Self {
            entries: Vec::new(),
            limit: MAX_STATES,
            timeouts,
        }
    }

    /// Build a table with an explicit limit (clamped to `1..=MAX_STATES`).
    #[must_use]
    pub fn with_limit(limit: usize, timeouts: Timeouts) -> Self {
        Self {
            entries: Vec::new(),
            limit: limit.clamp(1, MAX_STATES),
            timeouts,
        }
    }

    /// The configured limit.
    #[must_use]
    pub fn limit(&self) -> usize {
        self.limit
    }

    /// Number of live entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when no entry is tracked.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Read-only entries, in creation order.
    #[must_use]
    pub fn entries(&self) -> &[StateEntry] {
        &self.entries
    }

    /// Remove every entry whose deadline is at or before `now`; returns the removed count.
    pub fn expire(&mut self, now: u64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|entry| entry.expires > now);
        before - self.entries.len()
    }

    /// True when a live state already owns `(address, port)`.
    #[must_use]
    pub fn in_use(&self, address: IpAddr, port: u16) -> bool {
        self.entries.iter().any(|entry| {
            let created = entry
                .translation
                .is_some_and(|t| t.source == address && t.source_port == port);
            let key = entry.key.source == address && entry.key.source_port == port;
            created || key
        })
    }

    /// Find a state for `packet`, refresh it and return the creating rule.
    pub fn lookup(&mut self, packet: &Packet<'_>, now: u64) -> Option<StateHit> {
        let (index, orientation) = self.entries.iter().enumerate().find_map(|(index, entry)| {
            if let Some(side) = entry.key.orientation(packet) {
                return Some((index, side));
            }
            entry
                .peer
                .as_ref()
                .and_then(|peer| peer.orientation(packet))
                .map(|side| (index, side))
        })?;
        let entry = self.entries.get_mut(index)?;
        if entry.expires <= now {
            self.entries.remove(index);
            return None;
        }
        match orientation {
            Orientation::Forward => {
                entry.forward_packets = entry.forward_packets.saturating_add(1);
                entry.forward_bytes = entry.forward_bytes.saturating_add(u64::from(packet.length));
            }
            Orientation::Reverse => {
                entry.reverse_packets = entry.reverse_packets.saturating_add(1);
                entry.reverse_bytes = entry.reverse_bytes.saturating_add(u64::from(packet.length));
                entry.bidirectional = true;
            }
        }
        entry.last_seen = now;
        update_tcp(entry, packet, orientation);
        let timeout = timeout_for(&self.timeouts, entry, packet.protocol, orientation);
        entry.expires = now.saturating_add(timeout);
        // The reverse rewrite restores the pre-translation tuple recorded in `key`
        // (donor `sk`); `translation` describes the forward wire tuple.
        let translation = match orientation {
            Orientation::Forward => entry.translation,
            Orientation::Reverse => entry.translation.map(|translation| Translation {
                rule: translation.rule,
                source: entry.key.destination,
                source_port: entry.key.destination_port,
                destination: entry.key.source,
                destination_port: entry.key.source_port,
            }),
        };
        Some(StateHit {
            rule: entry.rule,
            orientation,
            tcp: entry.tcp,
            translation,
        })
    }

    /// Create a state for a passing packet, or return the existing disposition.
    ///
    /// The caller has already checked [`StateTable::lookup`]; this method is for the rule path.
    /// A full table returns [`StateDisposition::Rejected`]; it never evicts a live connection
    /// (donor `pf_create_state()` checks `max_states` first and drops).
    pub fn establish(
        &mut self,
        rule: usize,
        packet: &Packet<'_>,
        interface: Option<&str>,
        now: u64,
    ) -> StateDisposition {
        self.establish_with(rule, packet, interface, None, now)
    }

    /// Create a state, recording the translation when the connection is translated.
    pub fn establish_with(
        &mut self,
        rule: usize,
        packet: &Packet<'_>,
        interface: Option<&str>,
        translation: Option<Translation>,
        now: u64,
    ) -> StateDisposition {
        let timeouts = self.timeouts;
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.key.orientation(packet).is_some())
        {
            entry.last_seen = now;
            let timeout = timeout_for(&timeouts, entry, packet.protocol, Orientation::Forward);
            entry.expires = now.saturating_add(timeout);
            return StateDisposition::Existing;
        }
        self.expire(now);
        if self.entries.len() >= self.limit {
            return StateDisposition::Rejected;
        }
        let key = StateKey::from_packet(packet, interface);
        let peer = translation.map(|t| StateKey::translated(packet, interface, t));
        let tcp = initial_tcp_state(packet);
        let mut entry = StateEntry {
            key,
            peer,
            rule,
            translation,
            created: now,
            last_seen: now,
            expires: 0,
            forward_packets: 1,
            forward_bytes: u64::from(packet.length),
            reverse_packets: 0,
            reverse_bytes: 0,
            tcp,
            fin_forward: false,
            fin_reverse: false,
            bidirectional: false,
        };
        update_tcp(&mut entry, packet, Orientation::Forward);
        let timeout = timeout_for(&timeouts, &entry, packet.protocol, Orientation::Forward);
        entry.expires = now.saturating_add(timeout);
        self.entries.push(entry);
        StateDisposition::Created
    }
}

fn timeout_for(
    timeouts: &Timeouts,
    entry: &StateEntry,
    protocol: Protocol,
    orientation: Orientation,
) -> u64 {
    match protocol {
        Protocol::Tcp => match entry.tcp {
            TcpState::First => timeouts.tcp_first,
            TcpState::Opening => timeouts.tcp_opening,
            TcpState::Established => timeouts.tcp_established,
            TcpState::FinWait => timeouts.tcp_fin_wait,
            TcpState::Closing => timeouts.tcp_closing,
            TcpState::Closed => timeouts.tcp_closed,
        },
        Protocol::Udp => {
            if entry.bidirectional {
                timeouts.udp_multiple
            } else if entry.forward_packets > 1 {
                timeouts.udp_single
            } else {
                timeouts.udp_first
            }
        }
        Protocol::Icmp | Protocol::Icmpv6 => {
            if orientation == Orientation::Reverse {
                timeouts.icmp_error
            } else {
                timeouts.icmp_first
            }
        }
        Protocol::Any => {
            if entry.bidirectional {
                timeouts.other_multiple
            } else if entry.forward_packets > 1 {
                timeouts.other_single
            } else {
                timeouts.other_first
            }
        }
    }
}

fn initial_tcp_state(packet: &Packet<'_>) -> TcpState {
    if packet.protocol != Protocol::Tcp {
        return TcpState::First;
    }
    if packet.tcp_flags & tcp_flag::RST != 0 {
        return TcpState::Closed;
    }
    if packet.tcp_flags & tcp_flag::SYN != 0 && packet.tcp_flags & tcp_flag::ACK == 0 {
        return TcpState::First;
    }
    if packet.tcp_flags & tcp_flag::SYN != 0 {
        return TcpState::Opening;
    }
    if packet.tcp_flags & tcp_flag::FIN != 0 {
        return TcpState::FinWait;
    }
    if packet.tcp_flags & tcp_flag::ACK != 0 {
        return TcpState::Established;
    }
    TcpState::First
}

fn update_tcp(entry: &mut StateEntry, packet: &Packet<'_>, orientation: Orientation) {
    if packet.protocol != Protocol::Tcp {
        return;
    }
    if packet.tcp_flags & tcp_flag::RST != 0 {
        entry.tcp = TcpState::Closed;
        return;
    }
    if packet.tcp_flags & tcp_flag::FIN != 0 {
        match orientation {
            Orientation::Forward => entry.fin_forward = true,
            Orientation::Reverse => entry.fin_reverse = true,
        }
        entry.tcp = if entry.fin_forward && entry.fin_reverse {
            TcpState::Closing
        } else {
            TcpState::FinWait
        };
        return;
    }
    if packet.tcp_flags & tcp_flag::SYN != 0 && packet.tcp_flags & tcp_flag::ACK != 0 {
        if entry.tcp == TcpState::First {
            entry.tcp = TcpState::Opening;
        }
        return;
    }
    if packet.tcp_flags & tcp_flag::ACK != 0 && entry.tcp != TcpState::Closed {
        entry.tcp = TcpState::Established;
    }
}

#[cfg(test)]
#[path = "state_ut.rs"]
mod tests;
