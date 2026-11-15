// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Assertions for the MAC address and filter model.

use super::*;

fn mac(text: &str) -> MacAddress {
    MacAddress::parse(text).expect("MAC address")
}

#[test]
fn an_address_parses_and_prints() {
    let address = mac("02:11:22:33:44:55");
    assert_eq!(address.octets(), [0x02, 0x11, 0x22, 0x33, 0x44, 0x55]);
    assert_eq!(address.to_string(), "02:11:22:33:44:55");
    assert!(address.is_unicast());
    assert!(!address.is_multicast());
    assert!(!address.is_broadcast());
}

#[test]
fn multicast_and_broadcast_are_recognised() {
    assert!(mac("01:00:5e:00:00:01").is_multicast());
    assert!(mac("ff:ff:ff:ff:ff:ff").is_broadcast());
    assert!(!mac("02:00:00:00:00:01").is_broadcast());
}

#[test]
fn malformed_addresses_are_rejected() {
    assert_eq!(MacAddress::parse("02:11:22:33:44"), Err(MacError::Format));
    assert_eq!(
        MacAddress::parse("02:11:22:33:44:55:66"),
        Err(MacError::Format)
    );
    assert_eq!(
        MacAddress::parse("02:11:22:33:44:5g"),
        Err(MacError::Format)
    );
    assert_eq!(
        MacAddress::parse("02:11:22:33:44:+1"),
        Err(MacError::Format)
    );
    assert_eq!(
        MacAddress::from_bytes(&[0x02, 0x11]),
        Err(MacError::Length(2))
    );
}

#[test]
fn crc32_matches_the_standard_vector() {
    assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(b""), 0);
}

#[test]
fn the_hash_bit_is_in_range_and_stable() {
    let bit = MacFilter::hash_bit(mac("01:00:5e:00:00:01"));
    assert!(bit < 64);
    assert_eq!(
        bit,
        MacFilter::hash_bit(mac("01:00:5e:00:00:01")),
        "the model hash must be deterministic"
    );
}

#[test]
fn exact_entries_accept_only_matching_unicast() {
    let own = mac("02:00:00:00:00:01");
    let other = mac("02:00:00:00:00:02");
    let mut filter = MacFilter::new();
    assert!(filter.add_exact(own));
    assert!(filter.accepts(own));
    assert!(filter.accepts(mac("ff:ff:ff:ff:ff:ff")));
    assert!(!filter.accepts(other));
    assert!(!filter.accepts(mac("01:00:5e:00:00:01")));
}

#[test]
fn multicast_uses_the_hash_table() {
    let group = mac("01:00:5e:00:00:01");
    let mut filter = MacFilter::new();
    assert!(!filter.accepts(group));
    filter.add_hash(group);
    assert!(filter.accepts(group));
    assert_eq!(
        filter.hash(),
        1u64 << usize::from(MacFilter::hash_bit(group))
    );
}

#[test]
fn the_modes_widen_the_filter() {
    let group = mac("01:00:5e:00:00:01");
    let other = mac("02:00:00:00:00:02");
    let own = mac("02:00:00:00:00:01");
    let mut filter = MacFilter::new();
    assert!(filter.add_exact(own));

    filter.set_mode(FilterMode::AllMulticast);
    assert!(filter.accepts(group));
    assert!(!filter.accepts(other));

    filter.set_mode(FilterMode::Promiscuous);
    assert!(filter.accepts(group));
    assert!(filter.accepts(other));

    filter.set_mode(FilterMode::Normal);
    assert!(!filter.accepts(group));
    assert!(filter.accepts(own));
}

#[test]
fn exact_entries_reject_duplicates_multicast_and_overflow() {
    let mut filter = MacFilter::new();
    let own = mac("02:00:00:00:00:01");
    assert!(filter.add_exact(own));
    assert!(!filter.add_exact(own));
    assert!(!filter.add_exact(mac("01:00:5e:00:00:01")));
    assert!(!filter.add_exact(mac("ff:ff:ff:ff:ff:ff")));
    for index in 2..=MAX_EXACT as u8 {
        let address = MacAddress::new([0x02, 0, 0, 0, 0, index]);
        assert!(filter.add_exact(address), "entry {index}");
    }
    assert_eq!(filter.exact_len(), MAX_EXACT);
    assert!(!filter.add_exact(MacAddress::new([0x02, 0, 0, 0, 0, 0xFF])));
}

#[test]
fn an_exact_entry_can_be_removed() {
    let first = mac("02:00:00:00:00:01");
    let second = mac("02:00:00:00:00:02");
    let mut filter = MacFilter::new();
    assert!(filter.add_exact(first));
    assert!(filter.add_exact(second));
    assert!(filter.remove_exact(first));
    assert_eq!(filter.exact_len(), 1);
    assert!(!filter.accepts(first));
    assert!(filter.accepts(second));
    assert!(!filter.remove_exact(first));
}
