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

fn part() -> PartitionRecord {
    let mut record = PartitionRecord {
        index: 1,
        start_lba: 34,
        sectors: 32768,
        name: [0u8; 40],
        name_len: 8,
    };
    record.name[..8].copy_from_slice(b"ufs-root");
    record
}

#[test]
fn parses_show_and_device() {
    assert_eq!(parse_args(["show", "block:0"]), Ok(Some("block:0")));
    assert_eq!(parse_args(["show"]), Ok(None));
    assert_eq!(parse_args([]), Ok(None));
}

#[test]
fn rejects_other_commands() {
    assert_eq!(parse_args(["list"]), Err(GpartError::UnknownCommand));
    assert_eq!(
        parse_args(["show", "a", "b"]),
        Err(GpartError::TooManyArguments)
    );
}

#[test]
fn renders_geometry_and_partition() {
    assert_eq!(
        show("block0", &info(), &[part()]),
        "Geom name: block0\nMediasize: 67108864\nSectorsize: 512\nScheme: GPT\n  start  size  index  name\n     34 32768       1  ufs-root\n"
    );
}

#[test]
fn empty_table_still_has_header() {
    assert!(show("block0", &info(), &[]).ends_with("  start  size  index  name\n"));
}
