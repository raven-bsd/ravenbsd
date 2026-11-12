// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Bounded IPv4 neighbour (ARP) cache.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use raven_abi::errno::Errno;

use crate::{Instant, Ipv4Addr};

#[derive(Clone, Debug)]
pub(crate) struct QueuedPacket {
    pub(crate) destination: Ipv4Addr,
    pub(crate) protocol: u8,
    pub(crate) payload: Vec<u8>,
}

/// One neighbour-cache entry.
#[derive(Clone, Debug)]
pub struct NeighborEntry {
    /// Resolved hardware address, if any.
    pub mac: Option<[u8; 6]>,
    /// Expiry time for a resolved entry.
    pub expires: Option<Instant>,
    /// ARP probe retries already sent.
    pub retries: u8,
    pub(crate) queued: Option<QueuedPacket>,
    last_used: u64,
}

impl NeighborEntry {
    /// Whether a packet is waiting for this resolution.
    #[must_use]
    pub fn has_queued(&self) -> bool {
        self.queued.is_some()
    }

    /// Whether this entry is resolved and still valid at `now`.
    #[must_use]
    pub fn is_resolved(&self, now: Instant) -> bool {
        self.mac.is_some() && self.expires.is_some_and(|expires| expires > now)
    }
}

/// A bounded neighbour cache that evicts the least-recently-used entry.
#[derive(Clone, Debug)]
pub struct NeighborCache {
    entries: BTreeMap<Ipv4Addr, NeighborEntry>,
    capacity: usize,
    tick: u64,
}

impl NeighborCache {
    /// Create a cache with at most `capacity` entries.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            capacity,
            tick: 0,
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
    pub fn get(&self, address: Ipv4Addr) -> Option<&NeighborEntry> {
        self.entries.get(&address)
    }

    /// Iterate over entries.
    pub fn iter(&self) -> impl Iterator<Item = (Ipv4Addr, &NeighborEntry)> {
        self.entries
            .iter()
            .map(|(address, entry)| (*address, entry))
    }

    /// Insert or refresh a resolved entry.
    pub fn insert_resolved(
        &mut self,
        address: Ipv4Addr,
        mac: [u8; 6],
        now: Instant,
        lifetime_ms: u64,
    ) {
        self.make_room(address);
        self.tick = self.tick.wrapping_add(1);
        let expires = Instant::from_millis(
            now.as_millis()
                .saturating_add(i64::try_from(lifetime_ms).unwrap_or(i64::MAX)),
        );
        self.entries.insert(
            address,
            NeighborEntry {
                mac: Some(mac),
                expires: Some(expires),
                retries: 0,
                queued: None,
                last_used: self.tick,
            },
        );
    }

    /// Remove an entry.
    pub fn remove(&mut self, address: Ipv4Addr) -> bool {
        self.entries.remove(&address).is_some()
    }

    #[allow(dead_code)] // Called by the IPv4 output path and the testutil hook.
    pub(crate) fn pending(
        &mut self,
        address: Ipv4Addr,
        packet: QueuedPacket,
        deadline: Instant,
    ) -> Result<(), Errno> {
        if let Some(entry) = self.entries.get_mut(&address)
            && entry.queued.is_some()
        {
            return Err(Errno::ENOBUFS);
        }
        self.make_room(address);
        self.tick = self.tick.wrapping_add(1);
        self.entries.insert(
            address,
            NeighborEntry {
                mac: None,
                expires: Some(deadline),
                retries: 0,
                queued: Some(packet),
                last_used: self.tick,
            },
        );
        Ok(())
    }

    pub(crate) fn refresh_probe(&mut self, address: Ipv4Addr, deadline: Instant) -> bool {
        match self.entries.get_mut(&address) {
            Some(entry) if entry.mac.is_none() => {
                entry.retries = entry.retries.saturating_add(1);
                entry.expires = Some(deadline);
                true
            }
            _ => false,
        }
    }

    pub(crate) fn take_queued(&mut self, address: Ipv4Addr) -> Option<QueuedPacket> {
        self.entries.get_mut(&address)?.queued.take()
    }

    pub(crate) fn fail(&mut self, address: Ipv4Addr) -> Option<QueuedPacket> {
        self.entries.remove(&address)?.queued
    }

    pub(crate) fn expire(&mut self, now: Instant) -> usize {
        let before = self.entries.len();
        self.entries
            .retain(|_, entry| entry.expires.is_none_or(|expires| expires > now));
        before - self.entries.len()
    }

    pub(crate) fn due_probes(&self, now: Instant) -> Vec<Ipv4Addr> {
        self.entries
            .iter()
            .filter(|(_, entry)| {
                entry.mac.is_none() && entry.expires.is_some_and(|expires| expires <= now)
            })
            .map(|(address, _)| *address)
            .collect()
    }

    fn make_room(&mut self, address: Ipv4Addr) {
        if self.entries.contains_key(&address) || self.entries.len() < self.capacity {
            return;
        }
        let oldest = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(address, _)| *address);
        if let Some(oldest) = oldest {
            self.entries.remove(&oldest);
        }
    }
}
