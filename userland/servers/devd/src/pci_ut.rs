// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the PCI match/order rules.

use alloc::string::String;
use alloc::vec::Vec;

use raven_proto_dev::{DeviceInfo, DriverSpec};

use crate::pci::match_drivers;

fn device(bus: u8, slot: u8, function: u8, vendor: u16, product: u16) -> DeviceInfo {
    DeviceInfo {
        bus,
        slot,
        function,
        vendor,
        device: product,
        ..DeviceInfo::default()
    }
}

fn driver(name: &str, vendor: u16, product: u16) -> DriverSpec {
    DriverSpec {
        name: String::from(name),
        vendor,
        device: product,
        compatible: None,
        exec: String::from("/sbin/test"),
        args: Vec::new(),
        cycles: 1,
    }
}

#[test]
fn every_function_gets_one_driver_in_name_order() {
    let devices = [
        device(0, 4, 0, 0x1af4, 0x1042),
        device(0, 3, 0, 0x1af4, 0x1041),
        device(0, 9, 0, 0x1234, 0x11e8),
    ];
    let drivers = [
        driver("network", 0x1af4, 0x1041),
        driver("disk", 0x1af4, 0x1042),
    ];
    let matched = match_drivers(&devices, &drivers);
    assert_eq!(matched.len(), 2);
    assert_eq!(matched[0].device.name(), "00:03.0");
    assert_eq!(matched[0].driver.name, "network");
    assert_eq!(matched[1].device.name(), "00:04.0");
    assert_eq!(matched[1].driver.name, "disk");
}

#[test]
fn first_matching_table_entry_claims_the_function() {
    let devices = [device(0, 3, 0, 0x1af4, 0x1041)];
    let drivers = [
        driver("first", 0x1af4, 0x1041),
        driver("second", 0x1af4, 0x1041),
    ];
    let matched = match_drivers(&devices, &drivers);
    assert_eq!(matched.len(), 1);
    assert_eq!(matched[0].driver.name, "first");
}

#[test]
fn platform_entries_do_not_match_pci() {
    let devices = [device(0, 3, 0, 0x1af4, 0x1041)];
    let mut platform = driver("genet", 0x1af4, 0x1041);
    platform.compatible = Some(String::from("brcm,bcm2711-genet-v5"));
    let drivers = [platform];
    let matched = match_drivers(&devices, &drivers);
    assert!(matched.is_empty());
}

#[test]
fn two_functions_of_the_same_device_get_two_instances() {
    let devices = [
        device(0, 3, 0, 0x1af4, 0x1041),
        device(0, 4, 0, 0x1af4, 0x1041),
    ];
    let drivers = [driver("network", 0x1af4, 0x1041)];
    let matched = match_drivers(&devices, &drivers);
    assert_eq!(matched.len(), 2);
    assert_eq!(matched[0].device.function, 0);
    assert_eq!(matched[1].device.function, 0);
    assert_eq!(matched[0].device.slot, 3);
    assert_eq!(matched[1].device.slot, 4);
}

#[test]
fn empty_inputs_match_nothing() {
    let matched = match_drivers(&[], &[]);
    assert!(matched.is_empty());
}
