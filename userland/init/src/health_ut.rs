// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the init-side boot-health record and marker encoding.

use super::{HEALTH_PATH, Health, MARKER_BYTES, STATE_HEALTHY, encode_marker, encode_write, parse};

#[test]
fn health_record_parses() {
    let health = parse("slot = \"b\"\ntoken = \"deadbeef\"\nlba = \"34\"\n").unwrap();
    assert_eq!(
        health,
        Health {
            slot: 1,
            token: 0xdead_beef,
            lba: 34,
        }
    );
}

#[test]
fn health_record_rejects_bad_input() {
    assert!(parse("slot = \"c\"\ntoken = \"1\"\nlba = \"34\"\n").is_none());
    assert!(parse("slot = \"a\"\ntoken = \"zz\"\nlba = \"34\"\n").is_none());
    assert!(parse("slot = \"a\"\ntoken = \"1\"\nlba = \"x\"\n").is_none());
    assert!(parse("slot = \"a\"\ntoken = \"1\"\n").is_none());
    assert!(parse("mystery = \"x\"\n").is_none());
}

#[test]
fn marker_has_the_loader_layout() {
    let sector = encode_marker(1, 0x1234_5678, STATE_HEALTHY);
    assert_eq!(&sector[..8], b"RAVENHLT");
    assert_eq!(&sector[8..10], &1u16.to_le_bytes());
    assert_eq!(sector[10], 1);
    assert_eq!(sector[11], STATE_HEALTHY);
    assert_eq!(&sector[12..16], &0x1234_5678u32.to_le_bytes());
    assert_eq!(sector.len(), MARKER_BYTES);
}

#[test]
fn write_request_carries_one_sector() {
    let sector = encode_marker(0, 7, STATE_HEALTHY);
    let request = encode_write(34, &sector);
    assert_eq!(&request[0..2], &1u16.to_le_bytes());
    assert_eq!(&request[2..4], &2u16.to_le_bytes());
    assert_eq!(&request[4..12], &34u64.to_le_bytes());
    assert_eq!(&request[12..16], &1u32.to_le_bytes());
    assert_eq!(&request[20..24], &(MARKER_BYTES as u32).to_le_bytes());
    assert_eq!(&request[24..], &sector);
    assert_eq!(HEALTH_PATH, "/etc/boot-health");
}
