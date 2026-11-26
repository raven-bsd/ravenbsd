// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! FreeBSD ISN and timestamp-offset vectors (K278).

use raven_tcpip::{IpAddr, Ipv4Addr, Ipv6Addr, IsnConn, IsnEntropy, IsnState};

const SECRET: [u8; 16] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
];

const TS_SECRET: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];

fn v4_conn() -> IsnConn {
    IsnConn {
        local: IpAddr::V4(Ipv4Addr::new([192, 0, 2, 1])),
        local_port: 40_000,
        remote: IpAddr::V4(Ipv4Addr::new([198, 51, 100, 2])),
        remote_port: 80,
    }
}

fn v6_conn() -> IsnConn {
    IsnConn {
        local: IpAddr::V6(Ipv6Addr::new([
            0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ])),
        local_port: 40_000,
        remote: IpAddr::V6(Ipv6Addr::new([
            0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
        ])),
        remote_port: 443,
    }
}

#[test]
fn a_fresh_state_reseeds_and_projects_from_zero() {
    let mut state = IsnState::new(1024, SECRET, TS_SECRET);
    let isn = state.next_isn(&v4_conn(), 1024, IsnEntropy::new(SECRET, 0));
    assert_eq!(isn, 0x86f3_8d07);
    assert_eq!(state.offset(), 1_048_576);
    assert_eq!(state.last_tick(), 1024);
    assert_eq!(state.last_reseed(), 1024);
}

#[test]
fn two_calls_in_one_tick_advance_the_offset_by_the_static_increment() {
    let mut state = IsnState::new(1024, SECRET, TS_SECRET);
    let conn = v4_conn();
    let _ = state.next_isn(&conn, 1024, IsnEntropy::new(SECRET, 0));
    let first_offset = state.offset();
    let isn = state.next_isn(&conn, 1024, IsnEntropy::new(SECRET, 0));
    assert_eq!(state.offset() - first_offset, 4_096);
    assert_eq!(isn, 0x86f3_9d07);
}

#[test]
fn the_random_increment_uses_the_low_twelve_bits() {
    let mut state = IsnState::new(1024, SECRET, TS_SECRET);
    let conn = v4_conn();
    let _ = state.next_isn(&conn, 1024, IsnEntropy::new(SECRET, 0));
    let first_offset = state.offset();
    let isn = state.next_isn(&conn, 1024, IsnEntropy::new(SECRET, 0x0abc));
    assert_eq!(state.offset() - first_offset, 4_096 + 0xabc);
    assert_eq!(isn, 0x86f3_a7c3);
}

#[test]
fn an_idle_gap_projects_the_offset_at_one_mib_per_second() {
    let mut state = IsnState::new(1024, SECRET, TS_SECRET);
    let conn = v4_conn();
    let _ = state.next_isn(&conn, 1024, IsnEntropy::new(SECRET, 0));
    let first_offset = state.offset();
    let isn = state.next_isn(&conn, 2048, IsnEntropy::new(SECRET, 0));
    assert_eq!(state.offset() - first_offset, 1_048_576);
    assert_eq!(isn, 0x8703_8d07);
}

#[test]
fn the_reseed_interval_is_strictly_greater_than_the_interval() {
    let mut state = IsnState::new(100, SECRET, TS_SECRET);
    state.set_reseed_interval_secs(10);
    let _ = state.next_isn(&v4_conn(), 100, IsnEntropy::new(SECRET, 0));
    assert_eq!(state.last_reseed(), 100);
    assert!(!state.reseed_due(1_100));
    assert!(state.reseed_due(1_101));

    let other = [0x55; 16];
    let _ = state.next_isn(&v4_conn(), 1_101, IsnEntropy::new(other, 0));
    assert_eq!(state.last_reseed(), 1_101);
}

#[test]
fn an_explicit_reseed_changes_the_keyed_hash() {
    let mut state = IsnState::new(1024, SECRET, TS_SECRET);
    let conn = v4_conn();
    let _ = state.next_isn(&conn, 1024, IsnEntropy::new(SECRET, 0));

    let mut old = state.clone();
    let mut new = state.clone();
    new.reseed(1024, [0x55; 16]);
    let old_isn = old.next_isn(&conn, 1024, IsnEntropy::new(SECRET, 0));
    let new_isn = new.next_isn(&conn, 1024, IsnEntropy::new([0x55; 16], 0));
    assert_ne!(old_isn, new_isn);
}

#[test]
fn the_timestamp_offset_uses_the_per_connection_flag() {
    let mut state = IsnState::new(1024, SECRET, TS_SECRET);
    assert_eq!(state.ts_offset(&v4_conn()), 0x26ea_c308);

    state.set_ts_offset_per_conn(false);
    assert_eq!(state.ts_offset(&v4_conn()), 0x19ab_a317);
    let mut other_port = v4_conn();
    other_port.local_port = 40_001;
    assert_eq!(state.ts_offset(&other_port), 0x19ab_a317);

    state.set_ts_offset_per_conn(true);
    assert_ne!(state.ts_offset(&other_port), 0x26ea_c308);
}

#[test]
fn the_keyed_hash_hashes_the_freebsd_field_order() {
    let state = IsnState::new(1024, SECRET, SECRET);
    assert_eq!(state.ts_offset(&v4_conn()), 0x86e3_8d07);

    let mut swapped_addresses = v4_conn();
    swapped_addresses.local = v4_conn().remote;
    swapped_addresses.remote = v4_conn().local;
    assert_eq!(state.ts_offset(&swapped_addresses), 0x3aea_e205);

    let mut swapped_ports = v4_conn();
    swapped_ports.local_port = v4_conn().remote_port;
    swapped_ports.remote_port = v4_conn().local_port;
    assert_eq!(state.ts_offset(&swapped_ports), 0xdcff_7348);
}

#[test]
fn ipv6_hashes_sixteen_address_bytes_per_endpoint() {
    let state = IsnState::new(1024, SECRET, SECRET);
    assert_eq!(state.ts_offset(&v6_conn()), 0x8c04_dd94);
}

#[test]
fn a_different_timestamp_key_changes_the_offset() {
    let state = IsnState::new(1024, SECRET, TS_SECRET);
    let other = IsnState::new(1024, SECRET, [0x55; 16]);
    assert_ne!(state.ts_offset(&v4_conn()), other.ts_offset(&v4_conn()));
}
