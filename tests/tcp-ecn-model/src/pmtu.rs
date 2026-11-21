// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Reference PMTU cache, RFC 1191/8200 decisions and the FreeBSD MSS clamp.

use std::collections::BTreeMap;

use raven_tcpip::{IPV4_FLOOR, IPV6_FLOOR, IpAddr, IpFamily, MssUpdate};

const PLATEAUS: [u32; 12] = [
    65_535, 32_000, 17_914, 8_166, 4_352, 2_002, 1_492, 1_280, 1_006, 508, 296, 68,
];

fn headers(family: IpFamily) -> u32 {
    match family {
        IpFamily::V4 => 40,
        IpFamily::V6 => 60,
    }
}

/// Reference bounded, expiring MTU cache.
#[derive(Clone, Debug)]
pub struct RefCache {
    entries: BTreeMap<IpAddr, (u32, i64)>,
    capacity: usize,
    lifetime_ms: i64,
}

impl RefCache {
    /// Build a reference cache with the same clamps as the product.
    pub fn new(capacity: usize, lifetime_ms: i64) -> Self {
        Self {
            entries: BTreeMap::new(),
            capacity: capacity.max(1),
            lifetime_ms: lifetime_ms.max(1),
        }
    }

    /// Look up a non-expired entry.
    pub fn get(&self, destination: IpAddr, now_ms: i64) -> Option<u32> {
        self.entries
            .get(&destination)
            .filter(|(_, expires)| *expires > now_ms)
            .map(|(mtu, _)| *mtu)
    }

    /// Insert or refresh, evicting the earliest-expiring entry when full.
    pub fn insert(&mut self, destination: IpAddr, mtu: u32, now_ms: i64) {
        if !self.entries.contains_key(&destination) && self.entries.len() >= self.capacity {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, (_, expires))| *expires)
                .map(|(address, _)| *address);
            if let Some(address) = oldest {
                self.entries.remove(&address);
            }
        }
        self.entries
            .insert(destination, (mtu, now_ms.saturating_add(self.lifetime_ms)));
    }

    /// Drop expired entries.
    pub fn expire(&mut self, now_ms: i64) -> usize {
        let before = self.entries.len();
        self.entries.retain(|_, (_, expires)| *expires > now_ms);
        before - self.entries.len()
    }

    /// Number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

fn ref_next_lower_mtu(ip_len: u32) -> u32 {
    PLATEAUS
        .iter()
        .copied()
        .find(|mtu| *mtu < ip_len)
        .unwrap_or(0)
}

/// Reference `next_pmtu`.
pub fn ref_next_pmtu(reported: u32, ip_len: u32, minmss: u32) -> u32 {
    let mut mtu = if reported != 0 {
        reported
    } else {
        ref_next_lower_mtu(ip_len)
    };
    let floor = minmss.saturating_add(40);
    if mtu < floor {
        mtu = floor;
    }
    mtu
}

/// Reference IPv6 `next_pmtu`.
pub fn ref_next_pmtu6(reported: u32) -> u32 {
    reported.max(IPV6_FLOOR)
}

/// Reference IPv4 need-fragmentation decision.
pub fn ref_on_icmp_frag_needed(
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
    let mtu = ref_next_pmtu(reported_mtu, ip_len, minmss);
    (mtu < maxseg.saturating_add(40)).then_some(mtu)
}

/// Reference IPv6 need-fragmentation decision.
pub fn ref_on_icmp6_frag_needed(
    quoted_seq: u32,
    snd_una: u32,
    snd_max: u32,
    reported_mtu: u32,
    maxseg: u32,
) -> Option<u32> {
    if quoted_seq < snd_una || quoted_seq >= snd_max {
        return None;
    }
    let mtu = ref_next_pmtu6(reported_mtu);
    (mtu < maxseg.saturating_add(60)).then_some(mtu)
}

/// Reference `mssopt`.
pub fn ref_mssopt(maxmtu: u32, hc_mtu: u32, default_mss: u32, family: IpFamily) -> u32 {
    if maxmtu == 0 && hc_mtu == 0 {
        return default_mss;
    }
    let mtu = if maxmtu != 0 && hc_mtu != 0 {
        maxmtu.min(hc_mtu)
    } else {
        maxmtu.max(hc_mtu)
    };
    mtu.saturating_sub(headers(family))
}

/// Reference `mss_update`/`tcp_mss`.
#[allow(clippy::too_many_arguments)] // Mirrors the FreeBSD inputs.
pub fn ref_mss_update(
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
    let min_protoh = headers(family);
    let mut offer = offer;
    if mtuoffer != u32::MAX {
        offer = mtuoffer as i32 - min_protoh as i32;
    }
    if maxmtu == 0 {
        return MssUpdate {
            maxseg: default_mss,
            sack_prohibit: default_mss < 536,
        };
    }
    if offer == 0 {
        offer = default_mss as i32;
    } else {
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
        sack_prohibit: maxseg < 536,
    }
}

/// Reference family floor.
pub fn ref_floor_for(destination: IpAddr) -> u32 {
    match destination {
        IpAddr::V4(_) => IPV4_FLOOR,
        IpAddr::V6(_) => IPV6_FLOOR,
    }
}
