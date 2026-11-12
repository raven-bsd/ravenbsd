// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the device records and driver table.

use super::*;
use alloc::vec;

#[test]
fn device_info_roundtrip() {
    let device = DeviceInfo {
        bus: 0,
        slot: 2,
        function: 0,
        class: 1,
        subclass: 0,
        prog_if: 0,
        revision: 1,
        vendor: 0x1234,
        device: 0x11e8,
        bars: [0xfe00_0004, 0, 0, 0, 0, 0],
        interrupt_line: 11,
        interrupt_pin: 1,
    };
    let mut bytes = [0u8; DEVICE_INFO_LEN];
    device.encode(&mut bytes);
    assert_eq!(DeviceInfo::decode(&bytes), Some(device));
}

#[test]
fn device_info_rejects_wrong_length() {
    assert_eq!(DeviceInfo::decode(&[0u8; DEVICE_INFO_LEN - 1]), None);
    assert_eq!(DeviceInfo::decode(&[0u8; DEVICE_INFO_LEN + 1]), None);
}

#[test]
fn device_name_is_stable() {
    let device = DeviceInfo {
        bus: 0,
        slot: 2,
        function: 1,
        ..DeviceInfo::default()
    };
    assert_eq!(device.name(), "00:02.1");
}

#[test]
fn driver_table_parses_one_entry() {
    let drivers = parse_drivers(
        r#"
[[driver]]
name = "dummy"
vendor = 0x1234
device = 0x11e8
exec = "/sbin/devd"
args = ["dummy", "--flag"]
cycles = 10
"#,
    )
    .unwrap();
    assert_eq!(drivers.len(), 1);
    assert_eq!(drivers[0].name, "dummy");
    assert_eq!(drivers[0].vendor, 0x1234);
    assert_eq!(drivers[0].device, 0x11e8);
    assert_eq!(drivers[0].args, vec!["dummy", "--flag"]);
    assert_eq!(drivers[0].cycles, 10);
}

#[test]
fn driver_table_parses_multiple_entries_and_comments() {
    let drivers = parse_drivers(
        r#"
# a comment
[[driver]]
name = "a"
vendor = 1
device = 2
exec = "/sbin/a"
args = []

[[driver]]
name = "b"
vendor = 3
device = 4
exec = "/sbin/b"
"#,
    )
    .unwrap();
    assert_eq!(drivers.len(), 2);
    assert_eq!(drivers[0].cycles, 1);
    assert_eq!(drivers[1].name, "b");
}

#[test]
fn driver_table_rejects_bad_keys_and_values() {
    assert!(parse_drivers("[[driver]]\nname = \"a\"\nunknown = 1\n").is_err());
    assert!(parse_drivers("[[driver]]\nname = a\n").is_err());
    assert!(parse_drivers("[[driver]]\nname = \"a\"\nvendor = nope\n").is_err());
    assert!(parse_drivers("name = \"a\"\n").is_err());
}

#[test]
fn driver_table_rejects_missing_required_fields() {
    assert!(
        parse_drivers("[[driver]]\nname = \"a\"\nvendor = 1\ndevice = 2\nexec = \"/x\"\n").is_ok()
    );
    assert!(parse_drivers("[[driver]]\nvendor = 1\ndevice = 2\nexec = \"/x\"\n").is_err());
    assert!(
        parse_drivers("[[driver]]\nname = \"a\"\nvendor = 1\ndevice = 2\nexec = \"x\"\n").is_err()
    );
    assert!(
        parse_drivers(
            "[[driver]]\nname = \"a\"\nvendor = 1\ndevice = 2\nexec = \"/x\"\ncycles = 0\n"
        )
        .is_err()
    );
}
