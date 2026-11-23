// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the ACPI walker and the Raven boot bundle.

use std::collections::BTreeMap;

use super::*;

fn seal(bytes: &mut [u8], checksum_at: usize) {
    let mut sum = 0u8;
    for (index, byte) in bytes.iter().enumerate() {
        if index != checksum_at {
            sum = sum.wrapping_add(*byte);
        }
    }
    bytes[checksum_at] = 0u8.wrapping_sub(sum);
}

fn table(signature: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let length = TABLE_HEADER + body.len();
    let mut bytes = vec![0u8; length];
    bytes[..4].copy_from_slice(signature);
    bytes[4..8].copy_from_slice(&(length as u32).to_le_bytes());
    bytes[8] = 2;
    bytes[10..16].copy_from_slice(b"RAVEN ");
    bytes[36..].copy_from_slice(body);
    seal(&mut bytes, 9);
    bytes
}

fn rsdp(xsdt: u64, rsdt: u32, revision: u8) -> Vec<u8> {
    let length = if revision >= 2 {
        RSDP_LENGTH_V2
    } else {
        RSDP_LENGTH_V1
    };
    let mut bytes = vec![0u8; length];
    bytes[..8].copy_from_slice(RSDP_SIGNATURE);
    bytes[9..15].copy_from_slice(b"RAVEN ");
    bytes[15] = revision;
    bytes[16..20].copy_from_slice(&rsdt.to_le_bytes());
    if revision >= 2 {
        bytes[20..24].copy_from_slice(&(length as u32).to_le_bytes());
        bytes[24..32].copy_from_slice(&xsdt.to_le_bytes());
        seal(&mut bytes[..RSDP_LENGTH_V1], 8);
        seal(&mut bytes[..RSDP_LENGTH_V2], 32);
    } else {
        seal(&mut bytes[..RSDP_LENGTH_V1], 8);
    }
    bytes
}

fn mcfg(base: u64, segment: u16, start_bus: u8, end_bus: u8) -> Vec<u8> {
    let mut body = [0u8; 8 + MCFG_ENTRY];
    body[8..16].copy_from_slice(&base.to_le_bytes());
    body[16..18].copy_from_slice(&segment.to_le_bytes());
    body[18] = start_bus;
    body[19] = end_bus;
    table(b"MCFG", &body)
}

#[test]
fn rsdp_revision_two_parses_the_xsdt() {
    let bytes = rsdp(0x1234_5678, 0x1000, 2);
    let parsed = parse_rsdp(&bytes).unwrap();
    assert_eq!(parsed.revision, 2);
    assert_eq!(parsed.rsdt, 0x1000);
    assert_eq!(parsed.xsdt, 0x1234_5678);
    assert_eq!(usize::from(parsed.length), RSDP_LENGTH_V2);
}

#[test]
fn rsdp_bad_checksum_is_rejected() {
    let mut bytes = rsdp(0x2000, 0, 2);
    bytes[16] ^= 0x80;
    assert_eq!(parse_rsdp(&bytes), Err(Error::Checksum));
}

#[test]
fn root_entries_walk_the_xsdt_in_order() {
    let mut body = Vec::new();
    body.extend_from_slice(&0x2000u64.to_le_bytes());
    body.extend_from_slice(&0x3000u64.to_le_bytes());
    let xsdt = table(b"XSDT", &body);
    let entries: Vec<u64> = root_entries(&xsdt, true).unwrap().collect();
    assert_eq!(entries, vec![0x2000, 0x3000]);
}

#[test]
fn root_entries_reject_a_truncated_payload() {
    let body = [0u8; 7];
    let rsdt = table(b"RSDT", &body);
    assert_eq!(
        root_entries(&rsdt, false).map(|_| ()).unwrap_err(),
        Error::Length
    );
}

#[test]
fn mcfg_window_decodes_the_first_allocation() {
    let bytes = mcfg(0x4010_0000, 0, 0, 255);
    let window = mcfg_window(&bytes).unwrap();
    assert_eq!(window.base, 0x4010_0000);
    assert_eq!(window.segment, 0);
    assert_eq!(window.start_bus, 0);
    assert_eq!(window.end_bus, 255);
}

#[test]
fn mcfg_checksum_mutation_invalidates_the_window() {
    let mut bytes = mcfg(0x4010_0000, 0, 0, 255);
    bytes[MCFG_FIRST_ENTRY] ^= 0x01;
    assert_eq!(mcfg_window(&bytes), None);
}

#[test]
fn blob_round_trips_the_mcfg_table() {
    let rsdp_bytes = rsdp(0x5000, 0, 2);
    let mcfg_bytes = mcfg(0x4010_0000, 0, 0, 255);
    let mut store = BTreeMap::new();
    store.insert(0x1000u64, rsdp_bytes.clone());
    store.insert(0x5000u64, mcfg_bytes.clone());
    let tables = [
        table_entry(RSDP_SIGNATURE, 0x1000, rsdp_bytes.len() as u32),
        table_entry4(b"MCFG", 0x5000, mcfg_bytes.len() as u32),
    ];
    let mut blob = [0u8; MAX_BLOB_BYTES];
    let length = write_blob(&mut blob, &tables, |entry, destination| {
        let Some(source) = store.get(&entry.physical) else {
            return false;
        };
        if source.len() != destination.len() {
            return false;
        }
        destination.copy_from_slice(source);
        true
    })
    .unwrap();
    let blob = &blob[..length];
    assert_eq!(read_u32(blob, 0), Some(BLOB_MAGIC));
    assert_eq!(blob_table(blob, b"MCFG"), Some(mcfg_bytes.as_slice()));
    assert_eq!(mcfg_window(blob_table(blob, b"MCFG").unwrap()), mcfg_window(&mcfg_bytes));
}

#[test]
fn blob_without_the_magic_has_no_tables() {
    let mut blob = [0u8; 64];
    assert_eq!(blob_table(&blob, b"MCFG"), None);
    blob[..4].copy_from_slice(&BLOB_MAGIC.to_le_bytes());
    blob[16..20].copy_from_slice(&1u32.to_le_bytes());
    assert_eq!(blob_table(&blob, b"MCFG"), None);
}

#[test]
fn short_table_is_rejected() {
    let bytes = [0u8; 20];
    assert_eq!(validate_table(&bytes, b"MCFG"), Err(Error::Short));
}

#[test]
fn table_length_reads_the_header_field() {
    let bytes = mcfg(0x8000_0000, 0, 1, 2);
    assert_eq!(table_length(&bytes), Some(bytes.len() as u32));
    assert_eq!(table_signature(&bytes), Some(*b"MCFG"));
}
