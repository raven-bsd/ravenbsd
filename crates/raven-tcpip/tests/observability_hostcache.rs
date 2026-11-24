// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Hostcache observability through `observability.rs` (K293).

use raven_tcpip::{
    DEFAULT_BUCKET_LIMIT, DEFAULT_HASHSIZE, HostKey, HostMetrics, Hostcache, IpAddr, Ipv4Addr,
    Ipv6Addr,
};

fn v4_key() -> HostKey {
    HostKey::new(
        IpAddr::V4(Ipv4Addr::new([192, 0, 2, 1])),
        40_000,
        IpAddr::V4(Ipv4Addr::new([198, 51, 100, 2])),
        80,
    )
}

fn v6_key() -> HostKey {
    HostKey::new(
        IpAddr::V6(Ipv6Addr::new([
            0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ])),
        40_000,
        IpAddr::V6(Ipv6Addr::new([
            0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
        ])),
        443,
    )
}

fn metrics(rtt: u32, mss: u32, mtu: u32) -> HostMetrics {
    HostMetrics {
        mtu,
        mss,
        ssthresh: 64_000,
        rtt,
        rttvar: 0,
        srtt: 0,
        cwnd: 0,
        sendpipe: 0,
        recvpipe: 0,
    }
}

#[test]
fn ipv4_and_ipv6_metrics_are_observable_and_count_hits() {
    let mut cache = Hostcache::new(0);
    let empty = cache.observability_info();
    assert_eq!((empty.hits, empty.misses, empty.entries), (0, 0, 0));

    cache.update(&v4_key(), &metrics(100, 1460, 1500), 10);
    cache.update(&v6_key(), &metrics(200, 1400, 1420), 20);
    let info = cache.observability_info();
    assert_eq!(info.entries, 2);
    assert_eq!(info.histogram.len(), DEFAULT_BUCKET_LIMIT + 1);
    assert_eq!(info.histogram.iter().sum::<usize>(), DEFAULT_HASHSIZE);
    assert!(
        info.histogram.iter().skip(1).sum::<usize>() >= 1,
        "the two keys occupy at least one non-empty bucket"
    );

    let v4 = cache
        .observability_metrics(&v4_key(), 30)
        .expect("IPv4 metrics");
    assert_eq!((v4.rtt, v4.mss, v4.mtu), (100, 1460, 1500));
    let v6 = cache
        .observability_metrics(&v6_key(), 30)
        .expect("IPv6 metrics");
    assert_eq!((v6.rtt, v6.mss, v6.mtu), (200, 1400, 1420));

    let info = cache.observability_info();
    assert_eq!((info.hits, info.misses), (2, 0));
}

#[test]
fn expiry_and_purge_are_observed() {
    let mut cache = Hostcache::new(0);
    cache.update(&v4_key(), &metrics(100, 1460, 1500), 0);
    assert!(
        cache.observability_metrics(&v4_key(), 3_600_000).is_none(),
        "an entry at its expiry deadline is a miss"
    );
    let info = cache.observability_info();
    assert_eq!((info.hits, info.misses, info.entries), (0, 1, 0));

    cache.update(&v4_key(), &metrics(100, 1460, 1500), 10);
    cache.update(&v6_key(), &metrics(200, 1400, 1420), 10);
    assert_eq!(
        cache.purge(3_600_010),
        2,
        "purge removes both expired entries"
    );
    let info = cache.observability_info();
    assert_eq!(info.entries, 0);
    assert_eq!((info.hits, info.misses), (0, 1), "purge keeps the counters");

    cache.update(&v4_key(), &metrics(100, 1460, 1500), 20);
    assert_eq!(cache.purge_all(), 1);
    assert_eq!(cache.observability_info().entries, 0);
}

#[test]
fn the_hit_miss_view_tracks_each_lookup() {
    let mut cache = Hostcache::new(0);
    cache.update(&v4_key(), &metrics(100, 1460, 1500), 0);
    assert!(cache.observability_metrics(&v4_key(), 10).is_some());
    assert!(cache.observability_metrics(&v4_key(), 10).is_some());
    assert!(cache.observability_metrics(&v6_key(), 10).is_none());
    let info = cache.observability_info();
    assert_eq!((info.hits, info.misses), (2, 1));
}
