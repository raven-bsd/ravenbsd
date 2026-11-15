// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for UMAC filter programming.

use super::*;
use crate::mac::MacAddress;

fn own() -> MacAddress {
    MacAddress::new([0x02, 0x00, 0x00, 0x00, 0x00, 0x42])
}

#[test]
fn broadcast_and_station_occupy_the_first_two_slots() {
    let table = FilterTable::new(own());
    assert_eq!(table.slot_count(), 2);
    let broadcast = table.write(0).expect("broadcast");
    assert_eq!((broadcast.addr0, broadcast.addr1), (0xffff, 0xffff_ffff));
    let station = table.write(1).expect("station");
    assert_eq!((station.addr0, station.addr1), (0x0200, 0x0000_0042));
    assert_eq!(table.write(2), None);
}

#[test]
fn mdf_words_encode_the_broadcom_layout() {
    let address = MacAddress::new([0xd8, 0x3a, 0xdd, 0x12, 0x34, 0x56]);
    assert_eq!(mdf_words(address), (0xd83a, 0xdd12_3456));
}

#[test]
fn multicast_addresses_follow_the_station_slot() {
    let mut table = FilterTable::new(own());
    let first = MacAddress::new([0x01, 0x00, 0x5e, 0x00, 0x00, 0x01]);
    let second = MacAddress::new([0x01, 0x00, 0x5e, 0x00, 0x00, 0x02]);
    table.set_multicast(&[first, second]).expect("multicast");
    assert_eq!(table.slot_count(), 4);
    let write = table.write(2).expect("first multicast");
    assert_eq!((write.addr0, write.addr1), (0x0100, 0x5e00_0001));
    let write = table.write(3).expect("second multicast");
    assert_eq!((write.addr0, write.addr1), (0x0100, 0x5e00_0002));
    assert_eq!(table.write(4), None);
}

#[test]
fn setting_a_new_list_replaces_the_old_one() {
    let mut table = FilterTable::new(own());
    table
        .set_multicast(&[MacAddress::new([0x01, 0, 0x5e, 0, 0, 1])])
        .expect("first list");
    table
        .set_multicast(&[MacAddress::new([0x01, 0, 0x5e, 0, 0, 2])])
        .expect("second list");
    assert_eq!(table.slot_count(), 3);
    let write = table.write(2).expect("multicast");
    assert_eq!(write.addr1, 0x5e00_0002);
    assert_eq!(table.write(3), None);
}

#[test]
fn clearing_multicast_returns_to_two_slots() {
    let mut table = FilterTable::new(own());
    table
        .set_multicast(&[MacAddress::new([0x01, 0, 0x5e, 0, 0, 1])])
        .expect("multicast");
    table.clear_multicast();
    assert_eq!(table.slot_count(), 2);
    assert_eq!(table.write(2), None);
}

#[test]
fn more_than_fifteen_multicast_addresses_is_rejected() {
    let addresses = [MacAddress::new([0x01, 0, 0x5e, 0, 0, 1]); MAX_MULTICAST + 1];
    let mut table = FilterTable::new(own());
    assert_eq!(
        table.set_multicast(&addresses),
        Err(FilterError::TooManyMulticast {
            provided: MAX_MULTICAST + 1,
            capacity: MAX_MULTICAST,
        })
    );
    assert_eq!(table.slot_count(), 2);
}

#[test]
fn mdf_ctrl_enables_the_top_contiguous_slots() {
    let mut table = FilterTable::new(own());
    assert_eq!(table.mdf_ctrl(), 0x1_8000);
    table
        .set_multicast(&[MacAddress::new([0x01, 0, 0x5e, 0, 0, 1])])
        .expect("multicast");
    assert_eq!(table.mdf_ctrl(), 0x1_c000);
    table
        .set_multicast(&[MacAddress::new([0x01, 0, 0x5e, 0, 0, 1]); MAX_MULTICAST])
        .expect("full table");
    assert_eq!(table.mdf_ctrl(), 0x1_ffff);
}

#[test]
fn promiscuous_mode_sets_the_umac_command_bit() {
    let mut table = FilterTable::new(own());
    assert_eq!(table.command_bits(), 0);
    assert!(!table.is_promiscuous());
    table.set_promiscuous(true);
    assert_eq!(table.command_bits(), UMAC_CMD_PROMISC);
    assert!(table.is_promiscuous());
    table.set_promiscuous(false);
    assert_eq!(table.command_bits(), 0);
}

#[test]
fn an_unused_slot_has_no_write() {
    let table = FilterTable::new(own());
    assert_eq!(table.write(0).map(|write| write.slot), Some(0));
    assert_eq!(table.write(1).map(|write| write.slot), Some(1));
    assert_eq!(table.write(2), None);
    assert_eq!(table.write(255), None);
}
