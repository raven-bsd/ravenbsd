// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn info() -> BlockInfo {
    BlockInfo {
        sector_size: 512,
        read_only: 0,
        sectors: 131072,
        reserved: 0,
    }
}

fn part(index: u32, start: u64, sectors: u64, name: &str) -> PartitionRecord {
    let mut record = PartitionRecord {
        index,
        start_lba: start,
        sectors,
        name: [0u8; 40],
        name_len: name.len() as u8,
    };
    record.name[..name.len()].copy_from_slice(name.as_bytes());
    record
}

#[test]
fn parses_no_header_and_devices() {
    let (names, devices) = parse_args(["-n", "block:0"]).expect("args");
    assert!(names);
    assert_eq!(devices, ["block:0"]);
}

#[test]
fn rejects_unknown_options() {
    assert_eq!(parse_args(["-x"]), Err(ParseError));
}

#[test]
fn renders_device_and_partitions() {
    let parts = [part(1, 34, 32768, "ufs-root")];
    assert_eq!(
        render("block0", &info(), &parts, true),
        "NAME SIZE SECTORS TYPE\nblock0 67108864 131072 disk\nblock0p1 16777216 32768 part ufs-root\n"
    );
}

#[test]
fn renders_without_header() {
    assert_eq!(
        render("block0", &info(), &[], false),
        "block0 67108864 131072 disk\n"
    );
}

#[test]
fn name_is_utf8() {
    let record = part(1, 34, 1, "root");
    assert_eq!(partition_name(&record), "root");
}
