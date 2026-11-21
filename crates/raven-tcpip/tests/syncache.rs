// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD syncache behaviour: buckets, limits, retransmit timers, duplicate SYN, ACK
//! promotion, RST validation and counters.

extern crate alloc;

#[path = "../src/syncache.rs"]
mod syncache;

use syncache::{
    AbortOutcome, AckRequest, Address, Endpoint, InsertOutcome, Key, PromoteOutcome, SynOptions,
    SynRequest, Syncache, SyncacheAction, SyncacheConfig,
};

fn endpoint(seed: u8, port: u16) -> Endpoint {
    Endpoint {
        address: Address::V4([10, 0, 0, seed]),
        port,
    }
}

fn key(seed: u8) -> Key {
    Key {
        local: endpoint(1, 80),
        remote: endpoint(seed, 40_000 + u16::from(seed)),
    }
}

fn syn(seed: u8, now: u64) -> (SynRequest, u64) {
    (
        SynRequest {
            key: key(seed),
            irs: 100,
            iss: 1_000 + u32::from(seed),
            receive_window: 65_535,
            options: SynOptions {
                mss: Some(1_460),
                window_scale: Some(7),
                sack_permitted: true,
                timestamp: Some(10),
                ecn: true,
                no_options: false,
            },
            ip_ttl: 64,
            ip_tos: 0,
        },
        now,
    )
}

fn ack(seed: u8, ack_number: u32, seq: u32) -> AckRequest {
    AckRequest {
        key: key(seed),
        seq,
        ack: ack_number,
        window: 65_535,
        timestamp: Some(10),
    }
}

fn small_cache(buckets: usize, bucket_limit: usize, cache_limit: usize) -> Syncache {
    Syncache::new(SyncacheConfig {
        buckets,
        bucket_limit,
        cache_limit,
        ..SyncacheConfig::default()
    })
}

#[test]
fn config_defaults_match_freebsd() {
    let config = SyncacheConfig::default();
    assert_eq!(config.buckets, 512);
    assert_eq!(config.bucket_limit, 30);
    assert_eq!(config.cache_limit, 512 * 30);
    assert_eq!(config.initial_rto_ms, 1_000);
    assert_eq!(config.max_retransmits, 3);
}

#[test]
fn a_syn_creates_an_entry_with_the_negotiated_values() {
    let mut cache = small_cache(4, 2, 8);
    let (request, now) = syn(2, 0);
    assert!(matches!(
        cache.insert(request, now),
        InsertOutcome::Inserted { .. }
    ));
    assert_eq!(cache.len(), 1);
    let entry = cache.lookup(&key(2)).expect("entry");
    assert_eq!(entry.iss, 1_002);
    assert_eq!(entry.irs, 100);
    assert_eq!(entry.receive_window, 65_535);
    assert_eq!(entry.peer_mss, Some(1_460));
    assert_eq!(entry.requested_s_scale, 7);
    assert_eq!(entry.requested_r_scale, 3);
    assert!(entry.window_scale);
    assert!(entry.sack);
    assert!(entry.ecn);
    assert_eq!(entry.timestamp, Some(10));
    assert_eq!(entry.ip_ttl, 64);
    assert_eq!(entry.next_timeout_ms, 1_000);
    assert_eq!(cache.stats().added, 1);
}

#[test]
fn zero_options_clear_negotiated_timestamp_and_sack() {
    let mut cache = small_cache(1, 4, 8);
    let (mut request, now) = syn(1, 0);
    request.options.no_options = true;
    cache.insert(request, now);
    let entry = cache.lookup(&key(1)).expect("entry");
    assert!(!entry.window_scale);
    assert!(!entry.sack);
    assert_eq!(entry.timestamp, None);
    assert_eq!(entry.requested_r_scale, 3);
}

#[test]
fn the_advertised_window_is_clipped_to_tcp_maxwin() {
    let mut cache = small_cache(1, 4, 8);
    let (mut request, now) = syn(1, 0);
    request.receive_window = 100_000;
    cache.insert(request, now);
    assert_eq!(cache.lookup(&key(1)).expect("entry").receive_window, 65_535);
}

#[test]
fn a_duplicate_syn_resets_the_timer_and_refreshes_the_timestamp() {
    let mut cache = small_cache(1, 4, 8);
    let (request, _) = syn(1, 0);
    cache.insert(request, 0);
    assert_eq!(cache.poll(1_000), vec![SyncacheAction::Retransmit(key(1))]);
    assert_eq!(cache.lookup(&key(1)).expect("entry").rxmits, 1);

    let (mut duplicate, _) = syn(1, 0);
    duplicate.options.timestamp = Some(20);
    assert_eq!(
        cache.insert(duplicate, 1_500),
        InsertOutcome::Duplicate {
            iss: 1_001,
            rxmits: 0,
        }
    );
    let entry = cache.lookup(&key(1)).expect("entry");
    assert_eq!(entry.rxmits, 0);
    assert_eq!(entry.next_timeout_ms, 2_500);
    assert_eq!(entry.timestamp, Some(20));
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.stats().duplicate_syn, 1);

    let (mut without_timestamp, _) = syn(1, 0);
    without_timestamp.options.timestamp = None;
    cache.insert(without_timestamp, 2_000);
    assert_eq!(cache.lookup(&key(1)).expect("entry").timestamp, None);
}

#[test]
fn the_bucket_limit_evicts_the_oldest_entry() {
    let mut cache = small_cache(1, 2, 8);
    for (seed, now) in [(1, 0), (2, 1)] {
        let (request, _) = syn(seed, now);
        cache.insert(request, now);
    }
    let (third, now) = syn(3, 2);
    cache.insert(third, now);
    assert_eq!(cache.len(), 2);
    assert!(cache.lookup(&key(1)).is_none());
    assert!(cache.lookup(&key(2)).is_some());
    assert!(cache.lookup(&key(3)).is_some());
    assert_eq!(cache.bucket_len(0), 2);
    assert_eq!(cache.entries().count(), 2);
    assert_eq!(cache.stats().bucket_overflow, 1);
    assert_eq!(cache.stats().dropped, 1);
}

#[test]
fn the_global_cache_limit_evicts_the_oldest_entry() {
    let mut cache = small_cache(1, 8, 2);
    for (seed, now) in [(1, 0), (2, 1)] {
        let (request, _) = syn(seed, now);
        cache.insert(request, now);
    }
    let (third, now) = syn(3, 2);
    assert_eq!(
        cache.insert(third, now),
        InsertOutcome::Inserted { bucket: 0 }
    );
    assert_eq!(cache.len(), 2);
    assert!(cache.lookup(&key(1)).is_none());
    assert!(cache.lookup(&key(2)).is_some());
    assert!(cache.lookup(&key(3)).is_some());
    assert_eq!(cache.stats().cache_overflow, 1);
}

#[test]
fn promotion_requires_the_matching_ack_and_sequence() {
    let mut cache = small_cache(1, 4, 8);
    let (request, _) = syn(1, 0);
    cache.insert(request, 0);
    assert_eq!(cache.promote(ack(1, 1_000, 101)), PromoteOutcome::BadAck);
    assert_eq!(cache.len(), 1);
    assert_eq!(
        cache.promote(ack(1, 1_002, 100)),
        PromoteOutcome::BadSequence
    );
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.stats().bad_ack, 1);
    assert_eq!(cache.stats().challenge, 1);

    let outcome = cache.promote(ack(1, 1_002, 101));
    let PromoteOutcome::Promoted { entry, window } = outcome else {
        panic!("expected promotion, got {outcome:?}");
    };
    assert_eq!(entry.iss, 1_001);
    assert_eq!(window, 65_535);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.stats().completed, 1);
    assert_eq!(cache.promote(ack(1, 1_001, 101)), PromoteOutcome::Missing);
}

#[test]
fn promotion_enforces_timestamp_negotiation() {
    let mut cache = small_cache(1, 4, 8);
    let (request, _) = syn(1, 0);
    cache.insert(request, 0);
    let mut no_timestamp = ack(1, 1_002, 101);
    no_timestamp.timestamp = None;
    assert_eq!(
        cache.promote(no_timestamp),
        PromoteOutcome::MissingTimestamp
    );
    assert_eq!(cache.len(), 1);
    let mut stale = ack(1, 1_002, 101);
    stale.timestamp = Some(9);
    assert_eq!(cache.promote(stale), PromoteOutcome::StaleTimestamp);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.stats().timestamp_drops, 2);
    assert!(matches!(
        cache.promote(ack(1, 1_002, 101)),
        PromoteOutcome::Promoted { .. }
    ));
}

#[test]
fn an_rst_must_match_the_receive_window() {
    let mut cache = small_cache(1, 4, 8);
    let (request, _) = syn(1, 0);
    cache.insert(request, 0);
    assert_eq!(cache.abort(&key(1), 100), AbortOutcome::Ignored);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.abort(&key(1), 200), AbortOutcome::Challenge);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.stats().bad_rst, 2);
    assert_eq!(cache.abort(&key(1), 101), AbortOutcome::Removed);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.stats().reset, 1);
}

#[test]
fn retransmit_backoff_matches_the_donor_and_expires_after_the_budget() {
    let mut cache = small_cache(1, 4, 8);
    let (request, _) = syn(1, 0);
    cache.insert(request, 0);
    assert_eq!(cache.next_deadline(), Some(1_000));
    assert!(cache.poll(999).is_empty());
    assert_eq!(cache.next_deadline(), Some(1_000));

    assert_eq!(cache.poll(1_000), vec![SyncacheAction::Retransmit(key(1))]);
    assert_eq!(cache.next_deadline(), Some(3_000));
    assert_eq!(cache.poll(3_000), vec![SyncacheAction::Retransmit(key(1))]);
    assert_eq!(cache.next_deadline(), Some(7_000));
    assert_eq!(cache.poll(7_000), vec![SyncacheAction::Retransmit(key(1))]);
    assert_eq!(cache.next_deadline(), Some(15_000));
    assert_eq!(cache.poll(15_000), vec![SyncacheAction::Retransmit(key(1))]);
    assert_eq!(cache.lookup(&key(1)).expect("entry").rxmits, 4);
    assert_eq!(cache.next_deadline(), Some(31_000));
    assert_eq!(cache.poll(31_000), vec![SyncacheAction::Expired(key(1))]);
    assert_eq!(cache.len(), 0);
    assert_eq!(cache.stats().retransmits, 4);
    assert_eq!(cache.stats().stale, 1);
}

#[test]
fn a_new_entry_has_the_shortest_deadline() {
    let mut cache = small_cache(2, 4, 8);
    let (first, _) = syn(1, 0);
    cache.insert(first, 0);
    let (second, _) = syn(2, 500);
    cache.insert(second, 500);
    assert_eq!(cache.next_deadline(), Some(1_000));
    assert_eq!(cache.clear(), 2);
    assert!(cache.is_empty());
    assert_eq!(cache.next_deadline(), None);
}

#[test]
fn ipv6_keys_hash_and_lookup() {
    let mut cache = small_cache(4, 4, 8);
    let request = SynRequest {
        key: Key {
            local: Endpoint {
                address: Address::V6([0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
                port: 443,
            },
            remote: Endpoint {
                address: Address::V6([0xfe, 0x80, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2]),
                port: 50_000,
            },
        },
        irs: 7,
        iss: 11,
        receive_window: 0,
        options: SynOptions {
            mss: None,
            window_scale: None,
            sack_permitted: false,
            timestamp: None,
            ecn: false,
            no_options: false,
        },
        ip_ttl: 255,
        ip_tos: 0,
    };
    let key = request.key;
    cache.insert(request, 0);
    assert!(cache.lookup(&key).is_some());
    assert_eq!(cache.len(), 1);
}

#[test]
fn the_bucket_limit_is_exactly_the_configured_value() {
    let mut cache = small_cache(1, 2, 8);
    for (seed, now) in [(1, 0), (2, 1)] {
        let (request, _) = syn(seed, now);
        cache.insert(request, now);
    }
    assert_eq!(cache.bucket_len(0), 2);
    let (third, now) = syn(3, 2);
    cache.insert(third, now);
    assert_eq!(cache.bucket_len(0), 2);
    assert_eq!(cache.len(), 2);
    assert_eq!(cache.stats().bucket_overflow, 1);
}

#[test]
fn promotion_accepts_only_iss_plus_one() {
    let mut cache = small_cache(1, 4, 8);
    let (request, _) = syn(1, 0);
    cache.insert(request, 0);
    assert_eq!(cache.promote(ack(1, 1_001, 101)), PromoteOutcome::BadAck);
    assert_eq!(cache.promote(ack(1, 1_003, 101)), PromoteOutcome::BadAck);
    assert!(matches!(
        cache.promote(ack(1, 1_002, 101)),
        PromoteOutcome::Promoted { .. }
    ));
    assert_eq!(cache.len(), 0);
}

#[test]
fn only_an_rst_at_irs_plus_one_aborts() {
    let mut cache = small_cache(1, 4, 8);
    let (request, _) = syn(1, 0);
    cache.insert(request, 0);
    assert_eq!(cache.abort(&key(1), 102), AbortOutcome::Challenge);
    assert_eq!(cache.len(), 1);
    assert_eq!(cache.abort(&key(1), 101), AbortOutcome::Removed);
    assert_eq!(cache.len(), 0);
}

#[test]
fn a_duplicate_syn_restarts_the_initial_rto_timer() {
    let mut cache = small_cache(1, 4, 8);
    let (request, _) = syn(1, 0);
    cache.insert(request, 0);
    assert_eq!(cache.poll(1_000), vec![SyncacheAction::Retransmit(key(1))]);
    assert_eq!(cache.next_deadline(), Some(3_000));
    let (duplicate, _) = syn(1, 0);
    cache.insert(duplicate, 1_500);
    assert_eq!(cache.next_deadline(), Some(2_500));
    assert_eq!(cache.lookup(&key(1)).expect("entry").rxmits, 0);
}

#[test]
fn stats_start_at_zero() {
    let cache = Syncache::new(SyncacheConfig::default());
    let stats = cache.stats();
    assert_eq!(stats.added, 0);
    assert_eq!(stats.duplicate_syn, 0);
    assert_eq!(stats.completed, 0);
    assert_eq!(stats.dropped, 0);
    assert_eq!(stats.bucket_overflow, 0);
    assert_eq!(stats.cache_overflow, 0);
    assert_eq!(stats.bad_ack, 0);
    assert_eq!(stats.bad_rst, 0);
}
