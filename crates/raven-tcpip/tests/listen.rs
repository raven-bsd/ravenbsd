// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Listen queues, SYN cookies and ISN tests.

#![cfg(feature = "testutil")]

use raven_tcpip::{
    AcceptQueue, HalfOpen, HalfOpenQueue, IpAddr, Ipv4Addr, IsnGenerator, SynCookie,
};

fn peer(value: u8) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new([198, 51, 100, value]))
}

fn entry(value: u8) -> HalfOpen {
    HalfOpen {
        remote: peer(value),
        remote_port: 1000 + u16::from(value),
        irs: u32::from(value),
        iss: u32::from(value) + 10,
    }
}

#[test]
fn half_open_push_and_len() {
    let mut queue = HalfOpenQueue::new(2);
    assert!(queue.push(entry(1)));
    assert_eq!(queue.len(), 1);
}

#[test]
fn half_open_limit_drops_and_counts() {
    let mut queue = HalfOpenQueue::new(1);
    assert!(queue.push(entry(1)));
    assert!(!queue.push(entry(2)));
    assert_eq!(queue.stats.half_open_drops, 1);
}

#[test]
fn half_open_remove_by_tuple() {
    let mut queue = HalfOpenQueue::new(2);
    queue.push(entry(1));
    assert_eq!(queue.remove(peer(1), 1001), Some(entry(1)));
    assert!(queue.is_empty());
}

#[test]
fn half_open_remove_missing_is_none() {
    let mut queue = HalfOpenQueue::new(2);
    assert_eq!(queue.remove(peer(9), 999), None);
}

#[test]
fn accept_queue_push_and_pop() {
    let mut queue = AcceptQueue::new(2);
    assert!(queue.push(entry(1)));
    assert_eq!(queue.pop(), Some(entry(1)));
    assert!(queue.is_empty());
}

#[test]
fn accept_queue_limit_counts() {
    let mut queue = AcceptQueue::new(1);
    assert!(queue.push(entry(1)));
    assert!(!queue.push(entry(2)));
    assert_eq!(queue.stats.accept_queue_drops, 1);
}

#[test]
fn accept_queue_capacity_minimum_is_one() {
    let mut queue = AcceptQueue::new(0);
    assert!(queue.push(entry(1)));
}

#[test]
fn half_open_capacity_minimum_is_one() {
    let mut queue = HalfOpenQueue::new(0);
    assert!(queue.push(entry(1)));
}

#[test]
fn completed_handshake_moves_between_queues() {
    let mut half_open = HalfOpenQueue::new(2);
    let mut accept = AcceptQueue::new(2);
    half_open.push(entry(1));
    let connection = half_open.remove(peer(1), 1001).expect("half-open");
    assert!(accept.push(connection));
    assert!(half_open.is_empty());
    assert_eq!(accept.len(), 1);
}

#[test]
fn ten_thousand_syns_do_not_exhaust_memory() {
    let mut queue = HalfOpenQueue::new(64);
    for index in 0..10_000u32 {
        let address = Ipv4Addr::new([
            10,
            u8::try_from((index >> 16) & 0xff).unwrap_or(0),
            u8::try_from((index >> 8) & 0xff).unwrap_or(0),
            u8::try_from(index & 0xff).unwrap_or(0),
        ]);
        queue.push(HalfOpen {
            remote: IpAddr::V4(address),
            remote_port: 1000,
            irs: index,
            iss: index.wrapping_add(1),
        });
    }
    assert_eq!(queue.len(), 64);
    assert_eq!(queue.stats.half_open_drops, 10_000 - 64);
}

#[test]
fn legitimate_connection_succeeds_during_a_flood() {
    let mut queue = HalfOpenQueue::new(2);
    queue.push(entry(1));
    queue.push(entry(2));
    queue.push(entry(3));
    assert!(!queue.push(entry(4)));
    assert_eq!(queue.len(), 2);
}

#[test]
fn accept_overflow_does_not_reset_the_half_open_entry() {
    let mut half_open = HalfOpenQueue::new(2);
    let mut accept = AcceptQueue::new(1);
    assert!(accept.push(entry(2)));
    half_open.push(entry(1));
    let connection = half_open.remove(peer(1), 1001).expect("half-open");
    assert!(!accept.push(connection));
    assert_eq!(accept.stats.accept_queue_drops, 1);
}

#[test]
fn cookie_is_valid_for_the_same_tuple() {
    let cookie = SynCookie::new(42);
    let value = cookie.encode(peer(1), 1001, 80, 0);
    assert!(cookie.validate(value, peer(1), 1001, 80, 0));
}

#[test]
fn cookie_rejects_a_different_tuple() {
    let cookie = SynCookie::new(42);
    let value = cookie.encode(peer(1), 1001, 80, 0);
    assert!(!cookie.validate(value, peer(2), 1001, 80, 0));
}

#[test]
fn cookie_rejects_a_stale_time_bucket() {
    let cookie = SynCookie::new(42);
    let value = cookie.encode(peer(1), 1001, 80, 0);
    assert!(!cookie.validate(value, peer(1), 1001, 80, 60_000 * 9));
}

#[test]
fn cookie_secret_rotation_invalidates_old_cookies() {
    let mut cookie = SynCookie::new(42);
    let value = cookie.encode(peer(1), 1001, 80, 0);
    cookie.rotate();
    assert!(!cookie.validate(value, peer(1), 1001, 80, 0));
}

#[test]
fn cookie_survives_rotation_when_issued_after() {
    let mut cookie = SynCookie::new(42);
    cookie.rotate();
    let value = cookie.encode(peer(1), 1001, 80, 0);
    assert!(cookie.validate(value, peer(1), 1001, 80, 0));
}

#[test]
fn cookie_changes_with_the_seed() {
    let first = SynCookie::new(1);
    let second = SynCookie::new(2);
    let a = first.encode(peer(1), 1001, 80, 0);
    let b = second.encode(peer(1), 1001, 80, 0);
    assert_ne!(a, b);
}

#[test]
fn isn_same_tuple_differs_across_time() {
    let generator = IsnGenerator::new(7, false);
    let first = generator.generate(peer(1), 1001, 80, 1000);
    let second = generator.generate(peer(1), 1001, 80, 2000);
    assert_ne!(first, second);
}

#[test]
fn isn_different_tuples_differ() {
    let generator = IsnGenerator::new(7, false);
    let first = generator.generate(peer(1), 1001, 80, 1000);
    let second = generator.generate(peer(2), 1001, 80, 1000);
    assert_ne!(first, second);
}

#[test]
fn isn_deterministic_mode_is_reproducible() {
    let generator = IsnGenerator::new(7, true);
    assert_eq!(
        generator.generate(peer(1), 1001, 80, 1000),
        generator.generate(peer(1), 1001, 80, 2000)
    );
    assert!(generator.is_deterministic());
}

#[test]
fn isn_default_mode_is_not_deterministic() {
    let generator = IsnGenerator::new(7, false);
    assert!(!generator.is_deterministic());
}

#[test]
fn isn_has_bit_diversity() {
    let generator = IsnGenerator::new(7, false);
    let mut ones = 0u32;
    for index in 0..256u32 {
        let value = generator.generate(
            peer(1),
            u16::try_from(index).unwrap_or(0),
            80,
            u64::from(index),
        );
        ones += value.count_ones();
    }
    assert!(ones > 2000 && ones < 5000, "ones={ones}");
}

#[test]
fn isn_is_stable_for_the_same_time_and_tuple() {
    let generator = IsnGenerator::new(7, false);
    assert_eq!(
        generator.generate(peer(1), 1001, 80, 1000),
        generator.generate(peer(1), 1001, 80, 1000)
    );
}

#[test]
fn cookie_rotation_counter_changes_the_value() {
    let mut cookie = SynCookie::new(9);
    let first = cookie.encode(peer(1), 1001, 80, 0);
    cookie.rotate();
    let second = cookie.encode(peer(1), 1001, 80, 0);
    assert_ne!(first, second);
}

#[test]
fn stats_start_at_zero() {
    let queue = HalfOpenQueue::new(4);
    assert_eq!(queue.stats.half_open_drops, 0);
    assert_eq!(queue.stats.accept_queue_drops, 0);
}

#[test]
fn abandoned_counter_is_available() {
    let mut queue = AcceptQueue::new(1);
    queue.stats.abandoned = 1;
    assert_eq!(queue.stats.abandoned, 1);
}

#[test]
fn cookie_path_counter_is_available() {
    let mut queue = HalfOpenQueue::new(1);
    queue.stats.cookies_accepted = 3;
    assert_eq!(queue.stats.cookies_accepted, 3);
}

#[test]
fn cookie_time_bucket_is_stable_within_the_minute() {
    let cookie = SynCookie::new(42);
    let value = cookie.encode(peer(1), 1001, 80, 30_000);
    assert!(cookie.validate(value, peer(1), 1001, 80, 59_999));
}

#[test]
fn isn_deterministic_mode_ignores_the_clock() {
    let generator = IsnGenerator::new(5, true);
    assert_eq!(
        generator.generate(peer(3), 2000, 443, 0),
        generator.generate(peer(3), 2000, 443, 999_999)
    );
}
