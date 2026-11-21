// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD TCP hostcache behaviour (K286).

use raven_tcpip::{
    DEFAULT_BUCKET_LIMIT, DEFAULT_EXPIRE_MS, DEFAULT_HASHSIZE, DEFAULT_PRUNE_MS, HostKey,
    HostMetrics, Hostcache, IpAddr, Ipv4Addr, Ipv6Addr,
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

fn metrics(rtt: u32, rttvar: u32, mss: u32, srtt: u32) -> HostMetrics {
    HostMetrics {
        mtu: 1_500,
        mss,
        ssthresh: 64_000,
        rtt,
        rttvar,
        srtt,
        cwnd: 10,
        sendpipe: 1_000,
        recvpipe: 2_000,
    }
}

#[test]
fn defaults_match_the_freebsd_hostcache() {
    let cache = Hostcache::new(0);
    assert_eq!(DEFAULT_HASHSIZE, 512);
    assert_eq!(DEFAULT_BUCKET_LIMIT, 30);
    assert_eq!(DEFAULT_EXPIRE_MS, 3_600_000);
    assert_eq!(DEFAULT_PRUNE_MS, 300_000);
    assert_eq!(cache.expire_ms(), 3_600_000);
    assert_eq!(cache.prune_ms(), 300_000);
    assert!(cache.is_empty());
    assert_eq!(cache.len(), 0);
}

#[test]
fn an_empty_cache_misses_and_getmtu_is_zero() {
    let mut cache = Hostcache::new(0);
    assert!(cache.get(&v4_key(), 0).is_none());
    assert_eq!(cache.get_mtu(&v4_key(), 0), 0);
    assert_eq!(cache.hits(), 0);
    assert_eq!(cache.misses(), 2);
}

#[test]
fn a_metrics_update_is_returned_by_get() {
    let mut cache = Hostcache::new(0);
    let key = v4_key();
    let expected = metrics(100, 50, 1_460, 80);
    cache.update(&key, &expected, 0);
    assert_eq!(cache.get(&key, 0), Some(expected));
    assert_eq!(cache.get_mtu(&key, 0), 1_500);
    assert_eq!(cache.hits(), 2);
    assert_eq!(cache.len(), 1);
}

#[test]
fn rtt_variance_mss_and_srtt_are_smoothed_fifty_fifty() {
    let mut cache = Hostcache::new(0);
    let key = v4_key();
    cache.update(&key, &metrics(100, 40, 1_400, 90), 0);
    cache.update(&key, &metrics(200, 80, 1_200, 150), 0);
    let stored = cache.get(&key, 0).expect("entry");
    assert_eq!(stored.rtt, 150);
    assert_eq!(stored.rttvar, 60);
    assert_eq!(stored.mss, 1_300);
    assert_eq!(stored.srtt, 120);
    assert_eq!(stored.mtu, 1_500, "MTU is replaced, not averaged");
}

#[test]
fn ipv4_and_ipv6_keys_are_distinct() {
    let mut cache = Hostcache::new(0);
    let v4 = v4_key();
    let v6 = v6_key();
    cache.update(
        &v4,
        &HostMetrics {
            mtu: 1_500,
            ..HostMetrics::default()
        },
        0,
    );
    cache.update(
        &v6,
        &HostMetrics {
            mtu: 9_000,
            ..HostMetrics::default()
        },
        0,
    );
    assert_eq!(cache.get_mtu(&v4, 0), 1_500);
    assert_eq!(cache.get_mtu(&v6, 0), 9_000);
    assert_eq!(cache.len(), 2);
}

#[test]
fn the_full_four_tuple_is_the_key() {
    let mut cache = Hostcache::new(0);
    let key = v4_key();
    cache.update(
        &key,
        &HostMetrics {
            mtu: 1_400,
            ..HostMetrics::default()
        },
        0,
    );
    assert!(cache.get(&key, 0).is_some());

    let mut local_port = key;
    local_port.local_port = 40_001;
    assert!(cache.get(&local_port, 0).is_none());

    let mut remote_port = key;
    remote_port.remote_port = 443;
    assert!(cache.get(&remote_port, 0).is_none());

    let mut local_address = key;
    local_address.local = IpAddr::V4(Ipv4Addr::new([192, 0, 2, 9]));
    assert!(cache.get(&local_address, 0).is_none());

    let mut remote_address = key;
    remote_address.remote = IpAddr::V4(Ipv4Addr::new([198, 51, 100, 9]));
    assert!(cache.get(&remote_address, 0).is_none());
}

#[test]
fn update_mtu_creates_then_updates_an_entry() {
    let mut cache = Hostcache::new(0);
    let key = v4_key();
    cache.update_mtu(&key, 1_400, 0);
    assert_eq!(cache.get_mtu(&key, 0), 1_400);
    cache.update_mtu(&key, 1_300, 0);
    assert_eq!(cache.get_mtu(&key, 0), 1_300);
    assert_eq!(cache.len(), 1);
}

#[test]
fn an_entry_expires_at_the_deadline() {
    let mut cache = Hostcache::with_config(0, 4, 2, 1_000, 100);
    let key = v4_key();
    cache.update(
        &key,
        &HostMetrics {
            mtu: 1_500,
            ..HostMetrics::default()
        },
        0,
    );
    assert_eq!(cache.get_mtu(&key, 999), 1_500);
    assert_eq!(cache.get_mtu(&key, 1_000), 0);
    assert_eq!(cache.len(), 0);
}

#[test]
fn purge_removes_expired_entries_and_purge_all_removes_everything() {
    let mut cache = Hostcache::with_config(0, 4, 2, 1_000, 100);
    let first = v4_key();
    let second = v6_key();
    cache.update(
        &first,
        &HostMetrics {
            mtu: 1_500,
            ..HostMetrics::default()
        },
        0,
    );
    cache.update(
        &second,
        &HostMetrics {
            mtu: 9_000,
            ..HostMetrics::default()
        },
        0,
    );
    assert_eq!(cache.purge(999), 0);
    assert_eq!(cache.purge(1_000), 2);
    assert!(cache.is_empty());

    cache.update(
        &first,
        &HostMetrics {
            mtu: 1_500,
            ..HostMetrics::default()
        },
        2_000,
    );
    cache.update(
        &second,
        &HostMetrics {
            mtu: 9_000,
            ..HostMetrics::default()
        },
        2_000,
    );
    assert_eq!(cache.purge_all(), 2);
    assert!(cache.is_empty());
}

#[test]
fn the_cache_is_bounded_by_bucket_and_total_limits() {
    let mut cache = Hostcache::with_config(0, 2, 2, 60_000, 1_000);
    for index in 0..5u16 {
        let mut key = v4_key();
        key.remote_port = 1_000 + index;
        cache.update(
            &key,
            &HostMetrics {
                mtu: 1_500,
                ..HostMetrics::default()
            },
            0,
        );
    }
    assert!(cache.len() <= 4, "hashsize * bucket_limit");
    assert!(cache.bucket_len(0) <= 2);
    assert!(cache.bucket_len(1) <= 2);
    assert!(cache.evictions() >= 1);
}

#[test]
fn a_single_bucket_cannot_exceed_the_bucket_limit() {
    let mut cache = Hostcache::with_config(0, 2, 2, 60_000, 1_000);
    // These three ports share bucket 0 under the FNV 4-tuple hash; the bucket cap must evict one.
    for port in [1_000u16, 1_002, 1_004] {
        let mut key = v4_key();
        key.remote_port = port;
        cache.update(
            &key,
            &HostMetrics {
                mtu: 1_500,
                ..HostMetrics::default()
            },
            0,
        );
    }
    assert!(cache.bucket_len(0) <= 2);
    assert!(cache.bucket_len(1) <= 2);
    assert!(cache.evictions() >= 1);
}

#[test]
fn the_histogram_covers_every_bucket_length() {
    let mut cache = Hostcache::with_config(0, 4, 3, 60_000, 1_000);
    for index in 0..4u16 {
        let mut key = v4_key();
        key.remote_port = 2_000 + index;
        cache.update(
            &key,
            &HostMetrics {
                mtu: 1_500,
                ..HostMetrics::default()
            },
            0,
        );
    }
    let histogram = cache.histogram();
    assert_eq!(histogram.len(), 4, "bucket_limit + 1");
    assert_eq!(histogram.iter().sum::<usize>(), cache.len());
    assert!(histogram.iter().all(|count| *count <= 4));
}

#[test]
fn hits_misses_and_evictions_are_counted() {
    let mut cache = Hostcache::with_config(0, 2, 2, 60_000, 1_000);
    let key = v4_key();
    cache.update(
        &key,
        &HostMetrics {
            mtu: 1_500,
            ..HostMetrics::default()
        },
        0,
    );
    assert_eq!(cache.get_mtu(&key, 0), 1_500);
    assert_eq!(cache.get_mtu(&v6_key(), 0), 0);
    assert_eq!(cache.hits(), 1);
    assert_eq!(cache.misses(), 1);
    assert_eq!(cache.evictions(), 0);

    for index in 0..4u16 {
        let mut other = v4_key();
        other.remote_port = 3_000 + index;
        cache.update(
            &other,
            &HostMetrics {
                mtu: 1_500,
                ..HostMetrics::default()
            },
            0,
        );
    }
    assert!(cache.evictions() >= 1);
}
