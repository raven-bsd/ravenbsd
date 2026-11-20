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
/// NetBSD `tcp_mssdflt` (RFC 1122 default).
pub const TCP_MSS_DEFAULT: u32 = 536;
/// NetBSD `tcp_minmss` (minimum accepted offer).
pub const TCP_MINMSS: u32 = 256;

/// Address family used by the NetBSD MSS helpers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IpFamily {
    /// IPv4.
    V4,
    /// IPv6.
    V6,
}

/// TCP/IP header bytes NetBSD subtracts from an interface MTU.
#[must_use]
pub fn mss_headers(family: IpFamily) -> u32 {
    match family {
        IpFamily::V4 => 20 + 20,
        IpFamily::V6 => 40 + 20,
    }
}

/// NetBSD `tcp_mss_to_advertise`: interface/in_maxmtu minus IP+TCP headers.
///
/// `in_maxmtu` is the largest MTU of any attached network. With `mss_ifmtu`, only the selected
/// interface MTU is used, matching `tcp_mss_ifmtu`.
#[must_use]
pub fn mss_to_advertise(
    if_mtu: u32,
    in_maxmtu: u32,
    family: IpFamily,
    mss_ifmtu: bool,
    mss_default: u32,
) -> u32 {
    let mut mss = if_mtu;
    if !mss_ifmtu {
        mss = mss.max(in_maxmtu);
    }
    let headers = mss_headers(family);
    if mss > headers {
        mss -= headers;
    } else {
        mss = 0;
    }
    mss.max(mss_default)
}

/// Result of NetBSD `tcp_mss_from_peer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PeerMss {
    /// `t_peermss` before option/header subtraction.
    pub peer_mss: u32,
    /// `t_segsz`, the segment payload actually used.
    pub segsz: u32,
    /// Send buffer rounded up to a segment, as NetBSD does when it grows the socket buffer.
    pub send_buffer: u32,
}

/// NetBSD `tcp_mss_from_peer`: peer offer, 256-byte floor, options and send-buffer cap.
#[must_use]
pub fn mss_from_peer(
    offer: u32,
    mss_default: u32,
    optlen: u32,
    ip_optlen: u32,
    send_buffer: u32,
) -> PeerMss {
    let peer_mss = if offer == 0 { mss_default } else { offer }.max(TCP_MINMSS);
    let mut segsz = peer_mss.saturating_sub(optlen).saturating_sub(ip_optlen);
    if send_buffer < segsz {
        segsz = send_buffer;
    }
    let rounded = if segsz == 0 {
        send_buffer
    } else {
        send_buffer.div_ceil(segsz).saturating_mul(segsz)
    };
    PeerMss {
        peer_mss,
        segsz,
        send_buffer: rounded,
    }
}

/// One queued ICMP PMTU report (`TF_PMTUD_PEND` fields).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PmtuPending {
    /// TCP sequence quoted by the ICMP message.
    pub th_seq: u32,
    /// Advertised next-hop MTU.
    pub nextmtu: u32,
    /// Quoted IP length.
    pub ip_len: u16,
    /// Quoted IP header length in 32-bit words.
    pub ip_hl: u8,
}

/// NetBSD `tcp_ctlinput` PMTU state per connection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PmtuPath {
    /// Largest packet ever sent (`t_pmtud_mtu_sent`).
    pub mtu_sent: u32,
    /// Largest acknowledged payload (`t_pmtud_mss_acked`).
    pub mss_acked: u32,
    /// Queued ICMP report, if any.
    pub pending: Option<PmtuPending>,
}

/// Result of one ICMP need-fragmentation report.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PmtuResult {
    /// The quote was stale, unmatched or advertised no smaller MTU.
    Ignored,
    /// Traditional PMTUD: the new MTU is valid for acknowledged data.
    Classic(u32),
    /// The MTU is below acknowledged data; defer it until the quoted segment is acknowledged.
    Pending(PmtuPending),
}

/// NetBSD `tcp_ctlinput` PRC_MSGSIZE logic.
///
/// `headers` is `tcp_hdrsz` (IP + TCP headers). Reports outside `[snd_una, snd_max]` or at or
/// above `mtu_sent` are dropped. A smaller MTU that still covers headers plus `mss_acked` is a
/// classic path update; otherwise it is queued with the NetBSD replace rule for an older quote.
#[must_use]
#[allow(clippy::too_many_arguments)] // Mirrors NetBSD tcp_ctlinput's quoted header fields.
pub fn on_icmp_pmtu(
    path: &mut PmtuPath,
    seq: u32,
    snd_una: u32,
    snd_max: u32,
    reported_mtu: u32,
    headers: u32,
    ip_len: u16,
    ip_hl: u8,
) -> PmtuResult {
    if seq < snd_una || seq > snd_max || reported_mtu >= path.mtu_sent {
        return PmtuResult::Ignored;
    }
    if reported_mtu >= headers.saturating_add(path.mss_acked) {
        path.pending = None;
        return PmtuResult::Classic(reported_mtu);
    }
    if let Some(pending) = path.pending
        && pending.th_seq < seq
    {
        return PmtuResult::Ignored;
    }
    let pending = PmtuPending {
        th_seq: seq,
        nextmtu: reported_mtu,
        ip_len,
        ip_hl,
    };
    path.pending = Some(pending);
    PmtuResult::Pending(pending)
}

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
