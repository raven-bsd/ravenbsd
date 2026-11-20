// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Path-MTU cache, classic PMTUD and PLPMTUD tests.

#![cfg(feature = "testutil")]

use raven_tcpip::{
    IPV4_FLOOR, IPV6_FLOOR, IpAddr, IpFamily, Ipv4Addr, Ipv6Addr, MssUpdate, MtuCache, Plpmtud,
    PlpmtudState, TCP_MSS_DEFAULT, floor_for, mss_headers, mss_update, mssopt, next_pmtu,
    next_pmtu6, on_icmp_frag_needed, on_icmp6_frag_needed, probe_sequence,
};

fn v4(value: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new([192, 0, 2, value]))
}

fn v6() -> IpAddr {
    IpAddr::V6(Ipv6Addr::new([
        0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
    ]))
}

#[test]
fn cache_roundtrip() {
    let mut cache = MtuCache::new(4, 1000);
    cache.insert(v4(1), 1400, 0);
    assert_eq!(cache.get(v4(1), 10), Some(1400));
}

#[test]
fn cache_entry_expires() {
    let mut cache = MtuCache::new(4, 1000);
    cache.insert(v4(1), 1400, 0);
    assert_eq!(cache.get(v4(1), 1001), None);
}

#[test]
fn cache_expiry_counts_entries() {
    let mut cache = MtuCache::new(4, 1000);
    cache.insert(v4(1), 1400, 0);
    cache.insert(v4(2), 1400, 0);
    assert_eq!(cache.expire(1001), 2);
    assert!(cache.is_empty());
}

#[test]
fn cache_is_bounded_and_evicts() {
    let mut cache = MtuCache::new(2, 1000);
    cache.insert(v4(1), 1400, 0);
    cache.insert(v4(2), 1400, 1);
    cache.insert(v4(3), 1400, 2);
    assert_eq!(cache.len(), 2);
}

#[test]
fn cache_keeps_destinations_separate() {
    let mut cache = MtuCache::new(2, 1000);
    cache.insert(v4(1), 1400, 0);
    cache.insert(v4(2), 1500, 0);
    assert_eq!(cache.get(v4(1), 1), Some(1400));
    assert_eq!(cache.get(v4(2), 1), Some(1500));
}

#[test]
fn floors_are_family_specific() {
    assert_eq!(floor_for(v4(9)), IPV4_FLOOR);
    assert_eq!(floor_for(v6()), IPV6_FLOOR);
}

#[test]
fn plpmtud_starts_at_base() {
    let pmtud = Plpmtud::new(1200, 1500, 10);
    assert_eq!(pmtud.state, PlpmtudState::Base);
    assert_eq!(pmtud.current, 1200);
}

#[test]
fn plpmtud_start_enters_searching() {
    let mut pmtud = Plpmtud::new(1200, 1500, 10);
    pmtud.start();
    assert_eq!(pmtud.state, PlpmtudState::Searching);
}

#[test]
fn plpmtud_probe_is_between_base_and_max() {
    let mut pmtud = Plpmtud::new(1200, 1500, 10);
    pmtud.start();
    let probe = pmtud.next_probe().expect("probe");
    assert!(probe > 1200 && probe < 1500);
}

#[test]
fn plpmtud_ack_moves_the_low_bound() {
    let mut pmtud = Plpmtud::new(1200, 1500, 10);
    pmtud.start();
    let probe = pmtud.next_probe().expect("probe");
    pmtud.on_probe_ack(probe);
    assert!(pmtud.current >= probe);
}

#[test]
fn plpmtud_loss_moves_the_high_bound() {
    let mut pmtud = Plpmtud::new(1200, 1500, 10);
    pmtud.start();
    let probe = pmtud.next_probe().expect("probe");
    pmtud.on_probe_loss(probe);
    assert!(pmtud.current <= probe);
}

#[test]
fn plpmtud_budget_exhaustion_is_an_error() {
    let mut pmtud = Plpmtud::new(1200, 9000, 1);
    pmtud.start();
    let _ = pmtud.next_probe();
    let _ = pmtud.next_probe();
    assert_eq!(pmtud.state, PlpmtudState::Error);
    assert_eq!(pmtud.current, 1200);
}

#[test]
fn plpmtud_all_acks_complete_at_max() {
    let mut pmtud = Plpmtud::new(1200, 1500, 20);
    pmtud.start();
    while let Some(size) = pmtud.next_probe() {
        pmtud.on_probe_ack(size);
    }
    assert_eq!(pmtud.state, PlpmtudState::SearchComplete);
    assert!(pmtud.current >= 1499);
}

#[test]
fn plpmtud_probe_sequence_is_monotonic() {
    let sizes = probe_sequence(1200, 9000, 30);
    assert!(sizes.windows(2).all(|window| window[0] < window[1]));
}

#[test]
fn plpmtud_reprobe_is_due_after_the_interval() {
    let mut pmtud = Plpmtud::new(1200, 1500, 10);
    pmtud.start();
    while let Some(size) = pmtud.next_probe() {
        pmtud.on_probe_ack(size);
    }
    assert!(pmtud.reprobe_due(0, 10_000, 5_000));
}

#[test]
fn plpmtud_reprobe_is_not_due_early() {
    let mut pmtud = Plpmtud::new(1200, 1500, 10);
    pmtud.start();
    while let Some(size) = pmtud.next_probe() {
        pmtud.on_probe_ack(size);
    }
    assert!(!pmtud.reprobe_due(0, 1000, 5000));
}

#[test]
fn plpmtud_probe_loss_does_not_change_state_to_base() {
    let mut pmtud = Plpmtud::new(1200, 1500, 10);
    pmtud.start();
    let probe = pmtud.next_probe().expect("probe");
    pmtud.on_probe_loss(probe);
    assert_ne!(pmtud.state, PlpmtudState::Error);
}

#[test]
fn plpmtud_base_is_never_below_one() {
    let pmtud = Plpmtud::new(0, 1500, 10);
    assert_eq!(pmtud.base, 1);
}

#[test]
fn plpmtud_max_never_below_base() {
    let pmtud = Plpmtud::new(1500, 100, 10);
    assert_eq!(pmtud.max, 1500);
}

#[test]
fn probe_budget_is_retained() {
    let pmtud = Plpmtud::new(1200, 1500, 7);
    assert_eq!(pmtud.probe_budget(), 7);
}

#[test]
fn cache_capacity_at_least_one() {
    let mut cache = MtuCache::new(0, 1000);
    cache.insert(v4(1), 1400, 0);
    assert_eq!(cache.len(), 1);
}

#[test]
fn cache_lifetime_at_least_one() {
    let mut cache = MtuCache::new(1, 0);
    cache.insert(v4(1), 1400, 0);
    assert_eq!(cache.get(v4(1), 1), None);
}

#[test]
fn a_1400_path_converges_within_budget() {
    let mut pmtud = Plpmtud::new(1200, 1500, 10);
    pmtud.start();
    while let Some(size) = pmtud.next_probe() {
        if size <= 1400 {
            pmtud.on_probe_ack(size);
        } else {
            pmtud.on_probe_loss(size);
        }
    }
    assert_eq!(pmtud.state, PlpmtudState::SearchComplete);
    assert_eq!(pmtud.current, 1400);
}

#[test]
fn freebsd_mssopt_uses_the_smaller_known_mtu() {
    assert_eq!(mss_headers(IpFamily::V4), 40);
    assert_eq!(mss_headers(IpFamily::V6), 60);
    assert_eq!(mssopt(1500, 0, TCP_MSS_DEFAULT, IpFamily::V4), 1460);
    assert_eq!(mssopt(1500, 9000, TCP_MSS_DEFAULT, IpFamily::V4), 1460);
    assert_eq!(mssopt(0, 9000, TCP_MSS_DEFAULT, IpFamily::V6), 8940);
    assert_eq!(mssopt(0, 0, TCP_MSS_DEFAULT, IpFamily::V4), TCP_MSS_DEFAULT);
}

#[test]
fn freebsd_mss_update_clamps_offer_path_and_buffer() {
    assert_eq!(
        mss_update(
            1460,
            u32::MAX,
            9000,
            0,
            TCP_MSS_DEFAULT,
            216,
            true,
            false,
            8192,
            IpFamily::V4
        ),
        MssUpdate {
            maxseg: 1460,
            sack_prohibit: false
        }
    );
    // `case -1` falls through to `default` and is rounded up to tcp_minmss.
    assert_eq!(
        mss_update(
            -1,
            u32::MAX,
            9000,
            0,
            TCP_MSS_DEFAULT,
            216,
            true,
            false,
            8192,
            IpFamily::V4
        ),
        MssUpdate {
            maxseg: 216,
            sack_prohibit: true
        }
    );
    assert_eq!(
        mss_update(
            100,
            u32::MAX,
            1500,
            0,
            TCP_MSS_DEFAULT,
            216,
            true,
            false,
            8192,
            IpFamily::V4
        )
        .maxseg,
        216
    );
    assert_eq!(
        mss_update(
            0,
            1400,
            1500,
            0,
            TCP_MSS_DEFAULT,
            216,
            true,
            false,
            8192,
            IpFamily::V4
        )
        .maxseg,
        1360
    );
    assert_eq!(
        mss_update(
            1460,
            u32::MAX,
            1500,
            0,
            TCP_MSS_DEFAULT,
            216,
            true,
            false,
            1000,
            IpFamily::V4
        )
        .maxseg,
        1000
    );
}

#[test]
fn freebsd_icmp_next_pmtu_and_quote_filter() {
    assert_eq!(next_pmtu(1200, 1500, 216), 1200);
    assert_eq!(next_pmtu(1, 1500, 216), 256);
    // No advertised MTU: walk the RFC 1191 plateau table (includes 1280).
    assert_eq!(next_pmtu(0, 1500, 216), 1492);
    assert_eq!(next_pmtu(0, 1400, 216), 1280);
    assert_eq!(next_pmtu(0, 100, 216), 256);
    assert_eq!(
        on_icmp_frag_needed(100, 100, 200, 1200, 1500, 1460, 216),
        Some(1200)
    );
    assert_eq!(
        on_icmp_frag_needed(100, 100, 200, 1500, 1500, 1460, 216),
        None
    );
    assert_eq!(
        on_icmp_frag_needed(99, 100, 200, 1200, 1500, 1460, 216),
        None
    );
    assert_eq!(
        on_icmp_frag_needed(200, 100, 200, 1200, 1500, 1460, 216),
        None
    );
}

#[test]
fn freebsd_icmp6_next_pmtu_and_quote_filter() {
    assert_eq!(next_pmtu6(1400), 1400);
    assert_eq!(next_pmtu6(1000), IPV6_FLOOR);
    assert_eq!(on_icmp6_frag_needed(100, 100, 200, 1400, 1460), Some(1400));
    // 1460 + 60 is not smaller than 1460 + 60.
    assert_eq!(on_icmp6_frag_needed(100, 100, 200, 1520, 1460), None);
    assert_eq!(on_icmp6_frag_needed(99, 100, 200, 1400, 1460), None);
    assert_eq!(on_icmp6_frag_needed(200, 100, 200, 1400, 1460), None);
}
