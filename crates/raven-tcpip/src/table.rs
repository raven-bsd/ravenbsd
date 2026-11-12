// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Keyed connection classification, ephemeral ports and a separate TIME-WAIT table.

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::vec::Vec;

use crate::IpAddr;

/// A TCP four-tuple.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConnectionKey {
    /// Local address.
    pub local: IpAddr,
    /// Local port.
    pub local_port: u16,
    /// Remote address.
    pub remote: IpAddr,
    /// Remote port.
    pub remote_port: u16,
}

/// Counters for classification and the TIME-WAIT table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TableStats {
    /// Connections in collision chains longer than the warning bound.
    pub long_chain_warnings: usize,
    /// TIME-WAIT entries recycled because the table was full.
    pub timewait_recycled: usize,
    /// TIME-WAIT entries expired.
    pub timewait_expired: usize,
}

/// A secret-keyed, bounded-chain connection table.
#[derive(Clone, Debug)]
pub struct ConnectionTable<T> {
    buckets: Vec<Vec<(ConnectionKey, T)>>,
    secret: u64,
    max_chain: usize,
    pub stats: TableStats,
}

impl<T> ConnectionTable<T> {
    /// Create a table with a bucket count (rounded up to a power of two).
    #[must_use]
    pub fn new(bucket_count: usize, secret: u64, max_chain: usize) -> Self {
        let mut count = bucket_count.max(1).next_power_of_two();
        count = count.min(1 << 20);
        Self {
            buckets: (0..count).map(|_| Vec::new()).collect(),
            secret: secret | 1,
            max_chain: max_chain.max(1),
            stats: TableStats::default(),
        }
    }

    /// Change the collision-chain bound used for later warnings.
    pub fn set_max_chain(&mut self, max_chain: usize) {
        self.max_chain = max_chain.max(1);
    }

    /// The bucket index for a key (diagnostics/tests).
    #[must_use]
    pub fn bucket(&self, key: &ConnectionKey) -> usize {
        let mut hash = self.secret ^ 0x9e37_79b9_7f4a_7c15;
        hash = mix(hash ^ u64::from(key.local_port));
        hash = mix(hash ^ u64::from(key.remote_port));
        hash = mix(hash ^ address_hash(key.local));
        hash = mix(hash ^ address_hash(key.remote));
        usize::try_from(hash & (self.buckets.len() as u64 - 1)).unwrap_or(0)
    }

    /// Insert or replace a connection.
    pub fn insert(&mut self, key: ConnectionKey, value: T) {
        let index = self.bucket(&key);
        let bucket = &mut self.buckets[index];
        if let Some(slot) = bucket.iter_mut().find(|(existing, _)| *existing == key) {
            slot.1 = value;
            return;
        }
        bucket.push((key, value));
        if bucket.len() > self.max_chain {
            self.stats.long_chain_warnings = self.stats.long_chain_warnings.saturating_add(1);
        }
    }

    /// Classify an exact four-tuple.
    pub fn get(&self, key: &ConnectionKey) -> Option<&T> {
        let index = self.bucket(key);
        self.buckets[index]
            .iter()
            .find(|(existing, _)| existing == key)
            .map(|(_, value)| value)
    }

    /// Remove an exact four-tuple.
    pub fn remove(&mut self, key: &ConnectionKey) -> Option<T> {
        let index = self.bucket(key);
        let bucket = &mut self.buckets[index];
        let position = bucket.iter().position(|(existing, _)| existing == key)?;
        Some(bucket.remove(position).1)
    }

    /// Number of stored connections.
    #[must_use]
    pub fn len(&self) -> usize {
        self.buckets.iter().map(Vec::len).sum()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn address_hash(address: IpAddr) -> u64 {
    let octets = match address {
        IpAddr::V4(address) => address.octets().to_vec(),
        IpAddr::V6(address) => address.octets().to_vec(),
    };
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in octets {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

fn mix(mut value: u64) -> u64 {
    value = value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

/// A listener selector: `None` means wildcard local address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ListenerKey {
    /// Bound port.
    pub port: u16,
    /// Specific local address, or wildcard.
    pub local: Option<IpAddr>,
}

/// Two-level listener lookup: exact four-tuple first, then specific address,
/// then wildcard.
#[derive(Clone, Debug, Default)]
pub struct ListenerTable<T> {
    entries: BTreeMap<ListenerKey, T>,
}

impl<T> ListenerTable<T> {
    /// Create an empty listener table.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
        }
    }

    /// Insert a listener.
    pub fn insert(&mut self, key: ListenerKey, value: T) {
        self.entries.insert(key, value);
    }

    /// Classify a destination address/port.
    pub fn classify(&self, destination: IpAddr, port: u16) -> Option<&T> {
        self.entries
            .get(&ListenerKey {
                port,
                local: Some(destination),
            })
            .or_else(|| self.entries.get(&ListenerKey { port, local: None }))
    }

    /// Number of listeners.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no listeners.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// RFC 6056 randomised ephemeral-port allocator.
#[derive(Clone, Debug)]
pub struct PortAllocator {
    start: u16,
    end: u16,
    state: u64,
    used: BTreeSet<u16>,
    /// Attempts in the last allocation.
    pub last_attempts: usize,
}

impl PortAllocator {
    /// Create an allocator for an inclusive range.
    #[must_use]
    pub fn new(start: u16, end: u16, seed: u64) -> Self {
        Self {
            start,
            end: end.max(start),
            state: seed | 1,
            used: BTreeSet::new(),
            last_attempts: 0,
        }
    }

    /// Mark a port as unusable.
    pub fn reserve(&mut self, port: u16) {
        self.used.insert(port);
    }

    /// Change the inclusive allocation range; reservations are kept.
    pub fn set_range(&mut self, start: u16, end: u16) {
        self.start = start;
        self.end = end.max(start);
    }

    /// Release a port.
    pub fn release(&mut self, port: u16) {
        self.used.remove(&port);
    }

    /// Allocate a port, or `None` when the range is exhausted.
    pub fn allocate(&mut self) -> Option<u16> {
        let count = u32::from(self.end) - u32::from(self.start) + 1;
        self.last_attempts = 0;
        for _ in 0..count {
            self.state = mix(self.state);
            let offset = u32::try_from(self.state % u64::from(count)).unwrap_or(0);
            let port = u16::try_from(u32::from(self.start) + offset).unwrap_or(self.start);
            self.last_attempts = self.last_attempts.saturating_add(1);
            if !self.used.contains(&port) {
                self.used.insert(port);
                return Some(port);
            }
        }
        // Random probing can miss the last free slot; fall back to a bounded
        // linear scan only when the range is (almost) full.
        for port in self.start..=self.end {
            self.last_attempts = self.last_attempts.saturating_add(1);
            if !self.used.contains(&port) {
                self.used.insert(port);
                return Some(port);
            }
        }
        None
    }
}

/// A separate bounded TIME-WAIT table with O(1) expiry.
#[derive(Clone, Debug)]
pub struct TimeWaitTable<T> {
    entries: BTreeMap<ConnectionKey, (T, i64)>,
    expiry: VecDeque<(i64, ConnectionKey)>,
    capacity: usize,
    pub stats: TableStats,
}

impl<T: Clone> TimeWaitTable<T> {
    /// Create a table with its own capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: BTreeMap::new(),
            expiry: VecDeque::new(),
            capacity: capacity.max(1),
            stats: TableStats::default(),
        }
    }

    /// Insert a TIME-WAIT entry; when full, recycle the oldest and return the recycled values.
    pub fn insert(&mut self, key: ConnectionKey, value: T, deadline_ms: i64) -> Vec<T> {
        let mut recycled = Vec::new();
        let replacing = self.entries.contains_key(&key);
        while !replacing && self.entries.len() >= self.capacity {
            let Some((deadline, oldest)) = self.expiry.pop_front() else {
                break;
            };
            if self
                .entries
                .get(&oldest)
                .is_some_and(|(_, current)| *current == deadline)
                && let Some((evicted, _)) = self.entries.remove(&oldest)
            {
                recycled.push(evicted);
                self.stats.timewait_recycled = self.stats.timewait_recycled.saturating_add(1);
            }
        }
        self.entries.insert(key, (value, deadline_ms));
        self.expiry.push_back((deadline_ms, key));
        recycled
    }

    /// Expire entries due at `now`, O(1) per entry.
    pub fn expire(&mut self, now_ms: i64) -> usize {
        self.expire_values(now_ms).len()
    }

    /// Expire entries due at `now` and return their values.
    pub fn expire_values(&mut self, now_ms: i64) -> Vec<T> {
        self.expire_entries(now_ms)
            .into_iter()
            .map(|(_, value)| value)
            .collect()
    }

    /// Expire entries due at `now` and return their keys and values.
    pub fn expire_entries(&mut self, now_ms: i64) -> Vec<(ConnectionKey, T)> {
        let mut expired = Vec::new();
        while let Some((deadline, key)) = self.expiry.front().copied() {
            if deadline > now_ms {
                break;
            }
            self.expiry.pop_front();
            if self
                .entries
                .get(&key)
                .is_some_and(|(_, current)| *current == deadline)
                && let Some((value, _)) = self.entries.remove(&key)
            {
                expired.push((key, value));
            }
        }
        self.stats.timewait_expired = self.stats.timewait_expired.saturating_add(expired.len());
        expired
    }

    /// The value of a live entry.
    #[must_use]
    pub fn get(&self, key: &ConnectionKey) -> Option<&T> {
        self.entries.get(key).map(|(value, _)| value)
    }

    /// The deadline of a live entry, in milliseconds.
    #[must_use]
    pub fn deadline(&self, key: &ConnectionKey) -> Option<i64> {
        self.entries.get(key).map(|(_, deadline)| *deadline)
    }

    /// Forget an entry whose connection left TIME-WAIT.
    pub fn remove(&mut self, key: &ConnectionKey) -> Option<T> {
        self.entries.remove(key).map(|(value, _)| value)
    }

    /// Change the capacity; the next insert recycles down to it.
    pub fn set_capacity(&mut self, capacity: usize) {
        self.capacity = capacity.max(1);
    }

    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The RFC 6191 reuse rule.
    #[must_use]
    pub fn can_reuse(
        &self,
        key: &ConnectionKey,
        seq: u32,
        last_seq: u32,
        timestamps_used: bool,
        timestamp_ok: bool,
    ) -> bool {
        if !self.entries.contains_key(key) {
            return true;
        }
        if timestamps_used {
            timestamp_ok
        } else {
            seq > last_seq
        }
    }
}

/// The documented rebind decision matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rebind {
    /// Binding is allowed.
    Allowed,
    /// Binding returns `EADDRINUSE`.
    AddrInUse,
}

/// Decide a rebind against an existing local port.
#[must_use]
pub fn rebind_decision(
    same_local: bool,
    existing_listener: bool,
    existing_tcp: bool,
    time_wait: bool,
    reuse_addr: bool,
) -> Rebind {
    if existing_listener {
        return Rebind::AddrInUse;
    }
    if time_wait && !reuse_addr {
        return Rebind::AddrInUse;
    }
    if existing_tcp && !same_local {
        return Rebind::AddrInUse;
    }
    Rebind::Allowed
}

/// What a TIME-WAIT entry needs to answer a late segment after its socket record is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TimeWaitInfo {
    /// Next sequence number this side would send.
    pub snd_nxt: u32,
    /// Next sequence number expected from the peer.
    pub rcv_nxt: u32,
    /// Peer initial sequence number, for the new-incarnation test.
    pub irs: u32,
    /// Window to advertise in the ACK.
    pub window: u16,
    /// Last peer timestamp, echoed in the ACK.
    pub ts_recent: u32,
    /// Whether timestamps were negotiated.
    pub timestamps: bool,
}

/// A TIME-WAIT table slot: the socket record that still carries the connection, or the
/// detached state once the record was reclaimed.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TimeWaitSlot {
    /// Raw socket id, meaningful while `info` is `None`.
    pub socket: u32,
    /// Detached state.
    pub info: Option<TimeWaitInfo>,
}
