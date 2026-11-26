// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//
// Ported from FreeBSD at donor revision 133344405f532bd54ff3629a77d4c4e062f01fdb:
//   sys/netinet/tcp_hostcache.c  tcp_hc_init(), tcp_hc_destroy(), tcp_hc_cmp(),
//                                tcp_hc_lookup(), tcp_hc_get(), tcp_hc_getmtu(),
//                                tcp_hc_updatemtu(), tcp_hc_update(),
//                                tcp_hc_purge_internal(), tcp_hc_purge()
//   sys/netinet/tcp_var.h        struct tcp_hc_metrics, hc_metrics
// BSD-3-Clause. The kernel hash table, SMR, UMA zone, locks, callout and sysctls are replaced by
// a deterministic sans-IO bucket vector with an explicit `now`; no globals, no locks.

//! FreeBSD TCP hostcache: per-host path metrics.
//!
//! The hostcache is a bounded, two-level table: a configurable number of hash buckets, each
//! holding at most `bucket_limit` entries in least-recently-updated order. Entries are keyed by
//! the full connection 4-tuple and carry RTT, RTT variance, MSS, MTU and congestion-pipe metrics.
//!
//! The FreeBSD donor compares only the foreign address (`tcp_hc_cmp`) and hashes only that
//! address. This port uses the ticket's full 4-tuple key; the divergence is recorded in the K286
//! report. Time is caller-supplied (`now_ms`); there is no callout and no global mutable state.

use alloc::vec;
use alloc::vec::Vec;

use crate::IpAddr;

/// FreeBSD `TCP_HOSTCACHE_HASHSIZE`.
pub const DEFAULT_HASHSIZE: usize = 512;
/// FreeBSD `TCP_HOSTCACHE_BUCKETLIMIT`.
pub const DEFAULT_BUCKET_LIMIT: usize = 30;
/// FreeBSD `TCP_HOSTCACHE_EXPIRE` (one hour) in milliseconds.
pub const DEFAULT_EXPIRE_MS: u64 = 3_600_000;
/// FreeBSD `TCP_HOSTCACHE_PRUNE` (five minutes) in milliseconds.
pub const DEFAULT_PRUNE_MS: u64 = 300_000;

const MAX_HASHSIZE: usize = 4_096;
const MAX_BUCKET_LIMIT: usize = 1_024;
const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// A connection 4-tuple used as the hostcache key.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostKey {
    /// Local address.
    pub local: IpAddr,
    /// Local port.
    pub local_port: u16,
    /// Remote address.
    pub remote: IpAddr,
    /// Remote port.
    pub remote_port: u16,
}

impl HostKey {
    /// Build a hostcache key.
    #[must_use]
    pub const fn new(local: IpAddr, local_port: u16, remote: IpAddr, remote_port: u16) -> Self {
        Self {
            local,
            local_port,
            remote,
            remote_port,
        }
    }
}

/// Per-host metrics from FreeBSD `struct hc_metrics`/`struct tcp_hc_metrics`.
///
/// `mss` and `srtt` are Raven extensions requested by the K286 contract; the FreeBSD hostcache
/// carries `hc_rtt`/`hc_rttvar` but not a separate MSS or smoothed RTT.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostMetrics {
    /// Path MTU (`hc_mtu`).
    pub mtu: u32,
    /// Path MSS (Raven extension).
    pub mss: u32,
    /// Slow-start threshold (`hc_ssthresh`).
    pub ssthresh: u32,
    /// Estimated round-trip time (`hc_rtt`).
    pub rtt: u32,
    /// RTT variance (`hc_rttvar`).
    pub rttvar: u32,
    /// Smoothed RTT (Raven extension).
    pub srtt: u32,
    /// Congestion window (`hc_cwnd`).
    pub cwnd: u32,
    /// Outbound delay-bandwidth product (`hc_sendpipe`).
    pub sendpipe: u32,
    /// Inbound delay-bandwidth product (`hc_recvpipe`).
    pub recvpipe: u32,
}

impl HostMetrics {
    fn merge(&mut self, new: &Self) {
        if new.mtu != 0 {
            self.mtu = new.mtu;
        }
        smooth(&mut self.mss, new.mss);
        smooth(&mut self.ssthresh, new.ssthresh);
        smooth(&mut self.rtt, new.rtt);
        smooth(&mut self.rttvar, new.rttvar);
        smooth(&mut self.srtt, new.srtt);
        smooth(&mut self.cwnd, new.cwnd);
        smooth(&mut self.sendpipe, new.sendpipe);
        smooth(&mut self.recvpipe, new.recvpipe);
    }
}

fn smooth(old: &mut u32, new: u32) {
    if new == 0 {
        return;
    }
    *old = if *old == 0 {
        new
    } else {
        u32::try_from((u64::from(*old) + u64::from(new)) / 2).unwrap_or(u32::MAX)
    };
}

#[derive(Clone)]
struct Entry {
    key: HostKey,
    metrics: HostMetrics,
    expires_at_ms: u64,
}

/// A bounded, deterministic TCP hostcache.
pub struct Hostcache {
    buckets: Vec<Vec<Entry>>,
    bucket_limit: usize,
    expire_ms: u64,
    prune_ms: u64,
    next_prune_ms: u64,
    len: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl Hostcache {
    /// Create a hostcache with the FreeBSD defaults.
    #[must_use]
    pub fn new(now_ms: u64) -> Self {
        Self::with_config(
            now_ms,
            DEFAULT_HASHSIZE,
            DEFAULT_BUCKET_LIMIT,
            DEFAULT_EXPIRE_MS,
            DEFAULT_PRUNE_MS,
        )
    }

    /// Create a hostcache with explicit bounds.
    ///
    /// `hashsize` and `bucket_limit` are clamped to a safe range; `prune_ms` is clamped to
    /// `expire_ms` as the FreeBSD sysctl does. The total entry cap is `hashsize * bucket_limit`.
    #[must_use]
    pub fn with_config(
        now_ms: u64,
        hashsize: usize,
        bucket_limit: usize,
        expire_ms: u64,
        prune_ms: u64,
    ) -> Self {
        let hashsize = hashsize.clamp(1, MAX_HASHSIZE);
        let bucket_limit = bucket_limit.clamp(1, MAX_BUCKET_LIMIT);
        let expire_ms = expire_ms.max(1);
        let prune_ms = prune_ms.max(1).min(expire_ms);
        Self {
            buckets: vec![Vec::new(); hashsize],
            bucket_limit,
            expire_ms,
            prune_ms,
            next_prune_ms: now_ms.saturating_add(prune_ms),
            len: 0,
            hits: 0,
            misses: 0,
            evictions: 0,
        }
    }

    /// Number of live entries.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// True when the cache has no entries.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Configured expiry in milliseconds.
    #[must_use]
    pub const fn expire_ms(&self) -> u64 {
        self.expire_ms
    }

    /// Configured prune interval in milliseconds.
    #[must_use]
    pub const fn prune_ms(&self) -> u64 {
        self.prune_ms
    }

    /// Number of lookup hits.
    #[must_use]
    pub const fn hits(&self) -> u64 {
        self.hits
    }

    /// Number of lookup misses.
    #[must_use]
    pub const fn misses(&self) -> u64 {
        self.misses
    }

    /// Number of entries evicted by the bucket or total cap.
    #[must_use]
    pub const fn evictions(&self) -> u64 {
        self.evictions
    }

    /// Length of one hash bucket.
    #[must_use]
    pub fn bucket_len(&self, index: usize) -> usize {
        self.buckets.get(index).map_or(0, Vec::len)
    }

    /// Histogram of bucket lengths: index `n` counts buckets holding `n` entries.
    #[must_use]
    pub fn histogram(&self) -> Vec<usize> {
        let mut histogram = vec![0usize; self.bucket_limit + 1];
        for bucket in &self.buckets {
            histogram[bucket.len().min(self.bucket_limit)] += 1;
        }
        histogram
    }

    /// Look up a 4-tuple and return its metrics.
    ///
    /// An entry past its expiry is removed and reported as a miss. Hits/misses are counted as
    /// the FreeBSD lookup does.
    pub fn get(&mut self, key: &HostKey, now_ms: u64) -> Option<HostMetrics> {
        let index = self.bucket_index(key);
        let Some(bucket) = self.buckets.get(index) else {
            self.misses = self.misses.saturating_add(1);
            return None;
        };
        let Some(position) = bucket.iter().position(|entry| entry.key == *key) else {
            self.misses = self.misses.saturating_add(1);
            return None;
        };
        if self.buckets[index][position].expires_at_ms <= now_ms {
            self.buckets[index].remove(position);
            self.len = self.len.saturating_sub(1);
            self.misses = self.misses.saturating_add(1);
            return None;
        }
        self.hits = self.hits.saturating_add(1);
        Some(self.buckets[index][position].metrics)
    }

    /// Look up a path MTU; `0` on a miss.
    pub fn get_mtu(&mut self, key: &HostKey, now_ms: u64) -> u32 {
        self.get(key, now_ms).map_or(0, |metrics| metrics.mtu)
    }

    /// Merge metrics into an entry, creating it when absent.
    ///
    /// A present entry is refreshed, moved to the front of its bucket and merged with the
    /// FreeBSD 50/50 smoothing (MTU is replaced). A new entry evicts the least-recently-updated
    /// tail when the bucket or total cap is reached.
    pub fn update(&mut self, key: &HostKey, metrics: &HostMetrics, now_ms: u64) {
        let index = self.bucket_index(key);
        let expires_at_ms = now_ms.saturating_add(self.expire_ms);
        let existing = self.buckets[index]
            .iter()
            .position(|entry| entry.key == *key);

        if let Some(position) = existing {
            if self.buckets[index][position].expires_at_ms > now_ms {
                let mut entry = self.buckets[index].remove(position);
                entry.metrics.merge(metrics);
                entry.expires_at_ms = expires_at_ms;
                self.buckets[index].insert(0, entry);
                return;
            }
            self.buckets[index].remove(position);
            self.len = self.len.saturating_sub(1);
        }

        let cache_limit = self.bucket_limit.saturating_mul(self.buckets.len());
        if self.buckets[index].len() >= self.bucket_limit || self.len >= cache_limit {
            if self.buckets[index].is_empty() {
                // The donor gives up when this bucket is empty and the global cap is reached.
                return;
            }
            self.buckets[index].pop();
            self.len = self.len.saturating_sub(1);
            self.evictions = self.evictions.saturating_add(1);
        }

        self.buckets[index].insert(
            0,
            Entry {
                key: *key,
                metrics: *metrics,
                expires_at_ms,
            },
        );
        self.len = self.len.saturating_add(1);
    }

    /// Update only the path MTU, creating the entry when absent.
    pub fn update_mtu(&mut self, key: &HostKey, mtu: u32, now_ms: u64) {
        let metrics = HostMetrics {
            mtu,
            ..HostMetrics::default()
        };
        self.update(key, &metrics, now_ms);
    }

    /// Remove every entry whose expiry is at or before `now`; returns the count.
    pub fn purge(&mut self, now_ms: u64) -> usize {
        let mut removed = 0;
        for bucket in &mut self.buckets {
            let before = bucket.len();
            bucket.retain(|entry| entry.expires_at_ms > now_ms);
            removed += before - bucket.len();
        }
        self.len = self.len.saturating_sub(removed);
        removed
    }

    /// Remove every entry; returns the count.
    pub fn purge_all(&mut self) -> usize {
        let removed = self.len;
        for bucket in &mut self.buckets {
            bucket.clear();
        }
        self.len = 0;
        removed
    }

    /// Run the periodic purge when `now` reaches the next prune interval.
    ///
    /// Returns the number of expired entries removed. The caller drives this from its timer;
    /// the FreeBSD callout is deliberately not part of this sans-IO module.
    pub fn prune(&mut self, now_ms: u64) -> usize {
        if now_ms < self.next_prune_ms {
            return 0;
        }
        let removed = self.purge(now_ms);
        self.next_prune_ms = now_ms.saturating_add(self.prune_ms);
        removed
    }

    fn bucket_index(&self, key: &HostKey) -> usize {
        let hash = hash_key(key);
        usize::try_from(hash % self.buckets.len() as u64).unwrap_or(0)
    }
}

fn hash_key(key: &HostKey) -> u64 {
    let mut hash = FNV_OFFSET;
    hash = mix(hash, &key.remote_port.to_be_bytes());
    hash = mix(hash, &key.local_port.to_be_bytes());
    hash = match key.remote {
        IpAddr::V4(address) => mix(hash, &address.octets()),
        IpAddr::V6(address) => mix(hash, &address.octets()),
    };
    match key.local {
        IpAddr::V4(address) => mix(hash, &address.octets()),
        IpAddr::V6(address) => mix(hash, &address.octets()),
    }
}

fn mix(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
#[path = "hostcache_ut.rs"]
mod tests;
