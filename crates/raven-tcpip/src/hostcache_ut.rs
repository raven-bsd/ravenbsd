// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the FreeBSD hostcache port (K286).

use super::*;
use crate::{IpAddr, Ipv4Addr};

fn v4_key() -> HostKey {
    HostKey::new(
        IpAddr::V4(Ipv4Addr::new([192, 0, 2, 1])),
        40_000,
        IpAddr::V4(Ipv4Addr::new([198, 51, 100, 2])),
        80,
    )
}

#[test]
fn the_bucket_hash_is_remote_port_local_port_remote_address_local_address() {
    let cache = Hostcache::new(0);
    // FNV-1a over 0050, 9c40, c6336402, c0000201; swapping the two ports moves this to 493.
    assert_eq!(cache.bucket_index(&v4_key()), 253);
}

#[test]
fn smooth_is_fifty_fifty_and_ignores_zero() {
    let mut value = 100;
    smooth(&mut value, 200);
    assert_eq!(value, 150);
    smooth(&mut value, 0);
    assert_eq!(value, 150);

    let mut zero = 0;
    smooth(&mut zero, 42);
    assert_eq!(zero, 42);
}

#[test]
fn with_config_clamps_the_bounds() {
    let cache = Hostcache::with_config(0, 0, 0, 100, 1_000);
    assert_eq!(cache.expire_ms(), 100);
    assert_eq!(cache.prune_ms(), 100);
    assert_eq!(cache.histogram().len(), 2, "bucket_limit 1 + 1");
    assert!(cache.is_empty());
}

#[test]
fn an_expired_entry_is_replaced_by_update() {
    let mut cache = Hostcache::with_config(0, 4, 2, 100, 50);
    let key = v4_key();
    cache.update(
        &key,
        &HostMetrics {
            mtu: 1_500,
            ..HostMetrics::default()
        },
        0,
    );
    cache.update(
        &key,
        &HostMetrics {
            mtu: 1_400,
            ..HostMetrics::default()
        },
        200,
    );
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.get_mtu(&key, 200), 1_400);
}

#[test]
fn prune_is_rate_limited_and_removes_at_the_expiry_deadline() {
    let mut cache = Hostcache::with_config(0, 4, 2, 100, 50);
    let key = v4_key();
    cache.update(
        &key,
        &HostMetrics {
            mtu: 1_500,
            ..HostMetrics::default()
        },
        0,
    );
    assert_eq!(cache.prune(49), 0);
    assert_eq!(cache.prune(50), 0, "due, but not expired");
    assert_eq!(cache.prune(100), 1, "expiry is inclusive");
    assert!(cache.is_empty());
}

#[test]
fn an_eviction_keeps_a_bucket_bounded() {
    let mut cache = Hostcache::with_config(0, 1, 1, 60_000, 1_000);
    let mut first = v4_key();
    first.remote_port = 1_000;
    let mut second = v4_key();
    second.remote_port = 2_000;
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
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.bucket_len(0), 1);
    assert_eq!(cache.evictions(), 1);
    assert_eq!(cache.get_mtu(&second, 0), 9_000);
}
