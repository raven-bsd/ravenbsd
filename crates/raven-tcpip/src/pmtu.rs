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

/// FreeBSD `tcp_mssdflt`.
pub const TCP_MSS_DEFAULT: u32 = 536;
/// FreeBSD `TCP_MINMSS`.
pub const TCP_MINMSS: u32 = 216;

/// Address family used by the FreeBSD MSS helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpFamily {
    /// IPv4 (`struct tcpiphdr`).
    V4,
    /// IPv6 (IP6 header + TCP header).
    V6,
}

/// IP plus TCP header bytes (`min_protoh` in `tcp_mss_update`).
#[must_use]
pub fn mss_headers(family: IpFamily) -> u32 {
    match family {
        IpFamily::V4 => 40,
        IpFamily::V6 => 60,
    }
}

/// FreeBSD `tcp_mssopt`: the MSS option for an outgoing SYN.
///
/// `maxmtu` is the interface/route MTU, `hc_mtu` the host-cache MTU (0 when absent), and
/// `default_mss` the family default. When both are known the smaller is used; when neither is
/// known the default is returned.
#[must_use]
pub fn mssopt(maxmtu: u32, hc_mtu: u32, default_mss: u32, family: IpFamily) -> u32 {
    if maxmtu == 0 && hc_mtu == 0 {
        return default_mss;
    }
    let mtu = if maxmtu != 0 && hc_mtu != 0 {
        maxmtu.min(hc_mtu)
    } else {
        maxmtu.max(hc_mtu)
    };
    mtu.saturating_sub(mss_headers(family))
}

/// Result of FreeBSD `tcp_mss_update` and the send-buffer part of `tcp_mss`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MssUpdate {
    /// `t_maxseg` after the update.
    pub maxseg: u32,
    /// Whether `TF2_PROC_SACK_PROHIBIT` is required (`maxseg < tcp_mssdflt`).
    pub sack_prohibit: bool,
}

/// FreeBSD `tcp_mss_update`/`tcp_mss` path and buffer calculation.
///
/// `offer` is the peer MSS option (0 when absent, -1 when no SYN has arrived), `mtuoffer` the
/// ICMP MTU (use `u32::MAX` when absent), `maxmtu` the route/interface MTU, `hc_mtu` the cached
/// path MTU, `send_buffer` `so_snd.sb_hiwat`. The result is clamped to at least 64 bytes and the
/// 536-byte default controls the SACK prohibition bit.
#[must_use]
#[allow(clippy::too_many_arguments)] // Mirrors the FreeBSD tcp_mss_update inputs.
pub fn mss_update(
    offer: i32,
    mtuoffer: u32,
    maxmtu: u32,
    hc_mtu: u32,
    default_mss: u32,
    minmss: u32,
    path_mtu_discovery: bool,
    local: bool,
    send_buffer: u32,
    family: IpFamily,
) -> MssUpdate {
    let min_protoh = mss_headers(family);
    let mut offer = offer;
    if mtuoffer != u32::MAX {
        // FreeBSD: `offer = mtuoffer - min_protoh;` with signed `offer`.
        offer = mtuoffer as i32 - min_protoh as i32;
    }
    if maxmtu == 0 {
        return MssUpdate {
            maxseg: default_mss,
            sack_prohibit: default_mss < TCP_MSS_DEFAULT,
        };
    }
    if offer == 0 {
        offer = default_mss as i32;
    } else {
        // FreeBSD: `case -1` falls through to `default`, rounding up to `tcp_minmss`.
        offer = offer.max(minmss as i32);
    }
    let mut mss = if hc_mtu != 0 {
        hc_mtu.min(maxmtu).saturating_sub(min_protoh)
    } else {
        let mut value = maxmtu.saturating_sub(min_protoh);
        if !path_mtu_discovery && !local {
            value = value.min(default_mss);
        }
        value
    };
    mss = mss.min(offer.max(0) as u32).max(64);
    if send_buffer < mss {
        mss = send_buffer;
    }
    let maxseg = mss.max(64);
    MssUpdate {
        maxseg,
        sack_prohibit: maxseg < TCP_MSS_DEFAULT,
    }
}

/// FreeBSD `tcp_next_pmtu`: advertised next-hop MTU, fallback table step, and minimum.
#[must_use]
pub fn next_pmtu(reported: u32, ip_len: u32, minmss: u32) -> u32 {
    let mut mtu = if reported != 0 {
        reported
    } else {
        next_lower_mtu(ip_len)
    };
    let floor = minmss.saturating_add(40);
    if mtu < floor {
        mtu = floor;
    }
    mtu
}

/// The IP MTU plateaus from FreeBSD `ip_icmp.c` `ip_next_mtu` (RFC 1191).
const MTU_PLATEAUS: [u32; 12] = [
    65535, 32000, 17914, 8166, 4352, 2002, 1492, 1280, 1006, 508, 296, 68,
];

fn next_lower_mtu(ip_len: u32) -> u32 {
    MTU_PLATEAUS
        .iter()
        .copied()
        .find(|mtu| *mtu < ip_len)
        .unwrap_or(0)
}

/// FreeBSD `tcp_ctlinput_with_port` need-fragmentation decision.
///
/// The quoted sequence must be in `[snd_una, snd_max)`. The new MTU is returned only when it is
/// smaller than `maxseg + struct tcpiphdr`; otherwise the report leaves the path unchanged.
#[must_use]
pub fn on_icmp_frag_needed(
    quoted_seq: u32,
    snd_una: u32,
    snd_max: u32,
    reported_mtu: u32,
    ip_len: u32,
    maxseg: u32,
    minmss: u32,
) -> Option<u32> {
    if quoted_seq < snd_una || quoted_seq >= snd_max {
        return None;
    }
    let mtu = next_pmtu(reported_mtu, ip_len, minmss);
    (mtu < maxseg.saturating_add(40)).then_some(mtu)
}

/// FreeBSD `tcp6_next_pmtu`: the advertised MTU, floored at `IPV6_MMTU`.
#[must_use]
pub fn next_pmtu6(reported: u32) -> u32 {
    reported.max(IPV6_FLOOR)
}

/// FreeBSD `tcp6_ctlinput_with_port` need-fragmentation decision.
///
/// The quoted sequence must be in `[snd_una, snd_max)` and the MTU must be smaller than
/// `maxseg + sizeof(struct tcphdr) + sizeof(struct ip6_hdr)`.
#[must_use]
pub fn on_icmp6_frag_needed(
    quoted_seq: u32,
    snd_una: u32,
    snd_max: u32,
    reported_mtu: u32,
    maxseg: u32,
) -> Option<u32> {
    if quoted_seq < snd_una || quoted_seq >= snd_max {
        return None;
    }
    let mtu = next_pmtu6(reported_mtu);
    (mtu < maxseg.saturating_add(mss_headers(IpFamily::V6))).then_some(mtu)
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

    /// The next probe size without consuming a probe from the budget.
    ///
    /// This lets the send path reject a probe that does not fit the current window without
    /// losing search budget; `next_probe` remains the consuming entry point.
    #[must_use]
    pub fn next_probe_size(&self) -> Option<u32> {
        if self.state != PlpmtudState::Searching || self.probes >= self.budget {
            return None;
        }
        if self.high <= self.low + 1 {
            if self.current < self.high {
                Some(self.high)
            } else {
                None
            }
        } else {
            Some((self.low + (self.high - self.low) / 2).max(self.low + 1))
        }
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
