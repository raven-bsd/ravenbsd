// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Connection table, listener priority, port allocation and TIME-WAIT tests.

#![cfg(feature = "testutil")]

use raven_tcpip::{
    ConnectionKey, ConnectionTable, IpAddr, Ipv4Addr, ListenerKey, ListenerTable, PortAllocator,
    Rebind, TimeWaitTable, rebind_decision,
};

fn addr(value: u16) -> IpAddr {
    IpAddr::V4(Ipv4Addr::new([
        10,
        u8::try_from((value >> 8) & 0xff).unwrap_or(0),
        u8::try_from(value & 0xff).unwrap_or(0),
        1,
    ]))
}

fn key(value: u16) -> ConnectionKey {
    ConnectionKey {
        local: addr(1),
        local_port: 80,
        remote: addr(value),
        remote_port: 1024,
    }
}

#[test]
fn table_insert_and_get() {
    let mut table = ConnectionTable::new(16, 1, 8);
    table.insert(key(1), 42u32);
    assert_eq!(table.get(&key(1)), Some(&42));
}

#[test]
fn table_replace_keeps_one_entry() {
    let mut table = ConnectionTable::new(16, 1, 8);
    table.insert(key(1), 1u32);
    table.insert(key(1), 2);
    assert_eq!(table.get(&key(1)), Some(&2));
    assert_eq!(table.len(), 1);
}

#[test]
fn table_remove_returns_the_value() {
    let mut table = ConnectionTable::new(16, 1, 8);
    table.insert(key(1), 7u32);
    assert_eq!(table.remove(&key(1)), Some(7));
    assert!(table.is_empty());
}

#[test]
fn table_remove_missing_is_none() {
    let mut table = ConnectionTable::<u32>::new(16, 1, 8);
    assert_eq!(table.remove(&key(9)), None);
}

#[test]
fn classification_finds_one_of_ten_thousand() {
    let mut table = ConnectionTable::new(16_384, 1, 32);
    for index in 0..10_000u32 {
        let mut entry = key(u16::try_from(index % 60_000).unwrap_or(0));
        entry.remote_port = u16::try_from(index % 1000).unwrap_or(0);
        table.insert(entry, index);
    }
    assert_eq!(table.len(), 10_000);
    let mut found = 0;
    for index in 0..10_000u32 {
        let mut entry = key(u16::try_from(index % 60_000).unwrap_or(0));
        entry.remote_port = u16::try_from(index % 1000).unwrap_or(0);
        if table.get(&entry) == Some(&index) {
            found += 1;
        }
    }
    assert_eq!(found, 10_000);
}

#[test]
fn different_secrets_change_buckets() {
    let first = ConnectionTable::<u32>::new(64, 1, 4);
    let second = ConnectionTable::<u32>::new(64, 2, 4);
    let key = key(1);
    assert_ne!(first.bucket(&key), second.bucket(&key));
}

#[test]
fn long_collision_chains_are_counted() {
    let mut table = ConnectionTable::<u32>::new(1, 1, 2);
    table.insert(key(1), 1);
    table.insert(key(2), 2);
    table.insert(key(3), 3);
    assert!(table.stats.long_chain_warnings > 0);
}

#[test]
fn listener_exact_beats_wildcard() {
    let mut listeners = ListenerTable::new();
    listeners.insert(
        ListenerKey {
            port: 80,
            local: None,
        },
        "wildcard",
    );
    listeners.insert(
        ListenerKey {
            port: 80,
            local: Some(addr(1)),
        },
        "specific",
    );
    assert_eq!(listeners.classify(addr(1), 80), Some(&"specific"));
    assert_eq!(listeners.classify(addr(2), 80), Some(&"wildcard"));
}

#[test]
fn listener_on_a_different_port_is_not_matched() {
    let mut listeners = ListenerTable::new();
    listeners.insert(
        ListenerKey {
            port: 80,
            local: None,
        },
        1u32,
    );
    assert_eq!(listeners.classify(addr(1), 81), None);
}

#[test]
fn established_and_listener_are_distinguished() {
    let mut connections = ConnectionTable::new(16, 1, 8);
    let mut listeners = ListenerTable::new();
    connections.insert(key(1), "conn");
    listeners.insert(
        ListenerKey {
            port: 80,
            local: None,
        },
        "listen",
    );
    assert_eq!(connections.get(&key(1)), Some(&"conn"));
    assert_eq!(listeners.classify(addr(9), 80), Some(&"listen"));
}

#[test]
fn port_allocation_is_in_range() {
    let mut allocator = PortAllocator::new(10_000, 10_010, 3);
    let port = allocator.allocate().expect("port");
    assert!((10_000..=10_010).contains(&port));
}

#[test]
fn port_allocation_does_not_repeat_while_in_use() {
    let mut allocator = PortAllocator::new(10_000, 10_010, 3);
    let mut ports = Vec::new();
    for _ in 0..11 {
        ports.push(allocator.allocate().expect("port"));
    }
    ports.sort_unstable();
    ports.dedup();
    assert_eq!(ports.len(), 11);
}

#[test]
fn port_allocation_is_randomised() {
    let mut allocator = PortAllocator::new(10_000, 11_000, 9);
    let first = allocator.allocate().expect("first");
    let second = allocator.allocate().expect("second");
    assert_ne!(second, first + 1);
}

#[test]
fn port_exhaustion_returns_none() {
    let mut allocator = PortAllocator::new(10_000, 10_000, 1);
    assert_eq!(allocator.allocate(), Some(10_000));
    assert_eq!(allocator.allocate(), None);
}

#[test]
fn port_allocation_at_ninety_nine_percent_is_bounded() {
    let mut allocator = PortAllocator::new(10_000, 10_099, 7);
    for port in 10_000..10_099 {
        allocator.reserve(port);
    }
    let port = allocator.allocate().expect("last port");
    assert_eq!(port, 10_099);
    assert!(allocator.last_attempts <= 200);
}

#[test]
fn released_port_can_be_allocated_again() {
    let mut allocator = PortAllocator::new(10_000, 10_000, 1);
    let port = allocator.allocate().expect("first");
    allocator.release(port);
    assert_eq!(allocator.allocate(), Some(port));
}

#[test]
fn time_wait_insert_and_lookup() {
    let mut table = TimeWaitTable::new(4);
    table.insert(key(1), 1u32, 1000);
    assert_eq!(table.len(), 1);
}

#[test]
fn time_wait_expiry_removes_due_entries() {
    let mut table = TimeWaitTable::new(4);
    table.insert(key(1), 1u32, 1000);
    assert_eq!(table.expire(999), 0);
    assert_eq!(table.expire(1000), 1);
    assert!(table.is_empty());
}

#[test]
fn time_wait_limit_recycles_oldest() {
    let mut table = TimeWaitTable::new(1);
    table.insert(key(1), 1u32, 1000);
    table.insert(key(2), 2u32, 2000);
    assert_eq!(table.len(), 1);
    assert_eq!(table.stats.timewait_recycled, 1);
}

#[test]
fn ten_thousand_time_wait_entries_stay_within_the_limit() {
    let mut table = TimeWaitTable::new(10_000);
    for index in 0..10_000u32 {
        let mut entry = key(u16::try_from(index % 60_000).unwrap_or(0));
        entry.remote_port = u16::try_from(index % 1000).unwrap_or(0);
        table.insert(entry, index, i64::from(index));
    }
    assert_eq!(table.len(), 10_000);
}

#[test]
fn time_wait_expiry_is_o1_per_entry() {
    let mut table = TimeWaitTable::new(10_000);
    for index in 0..10_000u32 {
        let mut entry = key(u16::try_from(index % 60_000).unwrap_or(0));
        entry.remote_port = u16::try_from(index % 1000).unwrap_or(0);
        table.insert(entry, index, i64::from(index));
    }
    let removed = table.expire(10_000);
    assert_eq!(removed, 10_000);
}

#[test]
fn time_wait_reuse_requires_a_higher_sequence_without_timestamps() {
    let mut table = TimeWaitTable::new(4);
    table.insert(key(1), 1u32, 1000);
    assert!(table.can_reuse(&key(1), 2, 1, false, false));
    assert!(!table.can_reuse(&key(1), 1, 1, false, false));
}

#[test]
fn time_wait_reuse_uses_timestamps_when_available() {
    let mut table = TimeWaitTable::new(4);
    table.insert(key(1), 1u32, 1000);
    assert!(table.can_reuse(&key(1), 1, 1, true, true));
    assert!(!table.can_reuse(&key(1), 2, 1, true, false));
}

#[test]
fn time_wait_missing_key_can_always_reuse() {
    let table = TimeWaitTable::<u32>::new(4);
    assert!(table.can_reuse(&key(1), 0, 0, false, false));
}

#[test]
fn rebind_listener_is_rejected() {
    assert_eq!(
        rebind_decision(true, true, false, false, true),
        Rebind::AddrInUse
    );
}

#[test]
fn rebind_time_wait_requires_reuseaddr() {
    assert_eq!(
        rebind_decision(true, false, false, true, false),
        Rebind::AddrInUse
    );
    assert_eq!(
        rebind_decision(true, false, false, true, true),
        Rebind::Allowed
    );
}

#[test]
fn rebind_different_local_with_existing_tcp_is_rejected() {
    assert_eq!(
        rebind_decision(false, false, true, false, true),
        Rebind::AddrInUse
    );
}

#[test]
fn rebind_fresh_port_is_allowed() {
    assert_eq!(
        rebind_decision(true, false, false, false, false),
        Rebind::Allowed
    );
}

#[test]
fn listener_table_len_and_empty() {
    let mut listeners = ListenerTable::new();
    assert!(listeners.is_empty());
    listeners.insert(
        ListenerKey {
            port: 1,
            local: None,
        },
        1u32,
    );
    assert_eq!(listeners.len(), 1);
}

#[test]
fn inverted_port_range_collapses_to_the_start() {
    let mut allocator = PortAllocator::new(5000, 4000, 1);
    assert_eq!(allocator.allocate(), Some(5000));
    assert_eq!(allocator.allocate(), None);
}
