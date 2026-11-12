// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Path-MTU cache, classic PMTUD validation and the RFC 8899 PLPMTUD search.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

use crate::IpAddr;

/// IPv4 minimum path MTU (RFC 1191).
pub const IPV4_FLOOR: u32 = 576;
/// IPv6 minimum path MTU (RFC 8200).
pub const IPV6_FLOOR: u32 = 1280;

/// One cached path-MTU entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MtuEntry {
    /// Discovered MTU.
    pub mtu: u32,
    /// Expiry time in milliseconds.
    pub expires_ms: i64,
}

/// A bounded, expiring per-destination MTU cache.
#[derive(Clone, Debug)]
pub struct MtuCache {
    entries: BTreeMap<IpAddr, MtuEntry>,
    capacity: usize,
    lifetime_ms: i64,
}

impl MtuCache {
    /// Create a cache with `capacity` entries and an entry lifetime.
    #[must_use]
    pub fn new(capacity: usize, lifetime_ms: i64) -> Self {
        Self {
            entries: BTreeMap::new(),
            capacity: capacity.max(1),
            lifetime_ms: lifetime_ms.max(1),
        }
    }

    /// Look up a non-expired MTU.
    #[must_use]
    pub fn get(&self, destination: IpAddr, now_ms: i64) -> Option<u32> {
        self.entries
            .get(&destination)
            .filter(|entry| entry.expires_ms > now_ms)
            .map(|entry| entry.mtu)
    }

    /// Insert or refresh an entry, evicting the earliest-expiring one when full.
    pub fn insert(&mut self, destination: IpAddr, mtu: u32, now_ms: i64) {
        if !self.entries.contains_key(&destination)
            && self.entries.len() >= self.capacity
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.expires_ms)
                .map(|(address, _)| *address)
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(
            destination,
            MtuEntry {
                mtu,
                expires_ms: now_ms.saturating_add(self.lifetime_ms),
            },
        );
    }

    /// Remove expired entries.
    pub fn expire(&mut self, now_ms: i64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, entry| entry.expires_ms > now_ms);
        before - self.entries.len()
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
}

/// The family floor for an address.
#[must_use]
pub fn floor_for(destination: IpAddr) -> u32 {
    match destination {
        IpAddr::V4(_) => IPV4_FLOOR,
        IpAddr::V6(_) => IPV6_FLOOR,
    }
}

/// Validate and clamp an ICMP-reported MTU against the quoted connection.
///
/// `quoted_matches` must be true only when the quoted header identifies a live
/// connection; otherwise the report is ignored.
#[must_use]
pub fn classic_update(
    cached: Option<u32>,
    reported: u32,
    quoted_matches: bool,
    destination: IpAddr,
) -> Option<u32> {
    if !quoted_matches {
        return cached;
    }
    let floor = floor_for(destination);
    let value = reported.max(floor);
    Some(cached.map_or(value, |old| old.min(value).max(floor)))
}

/// The RFC 8899 PLPMTUD state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlpmtudState {
    /// Using the base MTU only.
    Base,
    /// Actively probing larger sizes.
    Searching,
    /// The largest working size has been found.
    SearchComplete,
    /// The search failed and fell back to the base MTU.
    Error,
}

/// A bounded RFC 8899 probe search.
#[derive(Clone, Debug)]
pub struct Plpmtud {
    /// Current state.
    pub state: PlpmtudState,
    /// Base MTU (safe minimum).
    pub base: u32,
    /// Interface MTU (upper bound).
    pub max: u32,
    /// Largest confirmed working MTU.
    pub current: u32,
    low: u32,
    high: u32,
    probe_size: u32,
    probes: usize,
    /// Maximum probes in one search.
    pub budget: usize,
}

impl Plpmtud {
    /// Start a search between `base` and `max`.
    #[must_use]
    pub fn new(base: u32, max: u32, budget: usize) -> Self {
        let base = base.max(1);
        let max = max.max(base);
        Self {
            state: PlpmtudState::Base,
            base,
            max: max.max(base),
            current: base,
            low: base,
            high: max.max(base),
            probe_size: base,
            probes: 0,
            budget: budget.max(1),
        }
    }

    /// Begin probing.
    pub fn start(&mut self) {
        self.state = PlpmtudState::Searching;
        self.low = self.base;
        self.high = self.max;
        self.probes = 0;
        self.probe_size = self.next_size();
    }

    fn next_size(&mut self) -> u32 {
        if self.high <= self.low + 1 {
            return self.high;
        }
        let mid = self.low + (self.high - self.low) / 2;
        mid.max(self.low + 1)
    }

    /// The next probe size, or `None` when the search is complete/error.
    pub fn next_probe(&mut self) -> Option<u32> {
        if self.state != PlpmtudState::Searching {
            return None;
        }
        if self.probes >= self.budget {
            self.state = PlpmtudState::Error;
            self.current = self.base;
            return None;
        }
        if self.high <= self.low + 1 {
            if self.current < self.high {
                self.probes += 1;
                self.probe_size = self.high;
                return Some(self.high);
            }
            self.state = PlpmtudState::SearchComplete;
            self.current = self.low;
            return None;
        }
        self.probes += 1;
        self.probe_size = self.next_size();
        Some(self.probe_size)
    }

    /// A probe of `size` was acknowledged.
    pub fn on_probe_ack(&mut self, size: u32) {
        self.low = size.max(self.low);
        self.current = self.low;
        if self.high <= self.low + 1 {
            self.state = PlpmtudState::SearchComplete;
        }
    }

    /// A probe of `size` was lost. Probe loss is never congestion loss.
    pub fn on_probe_loss(&mut self, size: u32) {
        self.high = size.min(self.high);
        if self.high <= self.low + 1 {
            self.state = PlpmtudState::SearchComplete;
            self.current = self.low;
        }
    }

    /// Whether the discovered size should be re-probed.
    #[must_use]
    pub fn reprobe_due(&self, last_probe_ms: i64, now_ms: i64, interval_ms: i64) -> bool {
        self.state == PlpmtudState::SearchComplete
            && now_ms.saturating_sub(last_probe_ms) >= interval_ms
    }

    /// The sizes probed in order.
    #[must_use]
    pub fn probe_budget(&self) -> usize {
        self.budget
    }
}

/// Probe sizes used by a search, for tests and diagnostics.
#[must_use]
pub fn probe_sequence(base: u32, max: u32, budget: usize) -> Vec<u32> {
    let mut plpmtud = Plpmtud::new(base, max, budget);
    plpmtud.start();
    let mut sizes = Vec::new();
    while let Some(size) = plpmtud.next_probe() {
        sizes.push(size);
        plpmtud.on_probe_ack(size);
    }
    sizes
}
