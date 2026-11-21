// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the classic-pcap reader/writer.

use super::*;

fn global_header(link_type: u32, magic: [u8; 4], snaplen: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&magic);
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&4u16.to_le_bytes());
    bytes.extend_from_slice(&0i32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&snaplen.to_le_bytes());
    bytes.extend_from_slice(&link_type.to_le_bytes());
    bytes
}

fn record(ts_sec: u32, ts_frac: u32, caplen: u32, orig_len: u32, data: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&ts_sec.to_le_bytes());
    bytes.extend_from_slice(&ts_frac.to_le_bytes());
    bytes.extend_from_slice(&caplen.to_le_bytes());
    bytes.extend_from_slice(&orig_len.to_le_bytes());
    bytes.extend_from_slice(data);
    bytes
}

#[test]
fn parses_classic_little_endian() {
    let mut bytes = global_header(1, [0xd4, 0xc3, 0xb2, 0xa1], 65535);
    bytes.extend_from_slice(&record(1_700_000_000, 123_456, 4, 4, &[1, 2, 3, 4]));
    let file = parse(&bytes).expect("valid pcap");
    assert_eq!(file.link_type, 1);
    assert_eq!(file.snaplen, 65535);
    assert_eq!(file.precision, TimestampPrecision::Microseconds);
    assert_eq!(file.packets.len(), 1);
    let packet = file.packets[0];
    assert_eq!(packet.ts_sec, 1_700_000_000);
    assert_eq!(packet.ts_frac, 123_456);
    assert_eq!(packet.data, &[1, 2, 3, 4]);
}

#[test]
fn parses_classic_big_endian() {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&[0xa1, 0xb2, 0xc3, 0xd4]);
    bytes.extend_from_slice(&2u16.to_be_bytes());
    bytes.extend_from_slice(&4u16.to_be_bytes());
    bytes.extend_from_slice(&0i32.to_be_bytes());
    bytes.extend_from_slice(&0u32.to_be_bytes());
    bytes.extend_from_slice(&65535u32.to_be_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&1u32.to_be_bytes());
    bytes.extend_from_slice(&2u32.to_be_bytes());
    bytes.extend_from_slice(&4u32.to_be_bytes());
    bytes.extend_from_slice(&4u32.to_be_bytes());
    bytes.extend_from_slice(&[9, 8, 7, 6]);
    let file = parse(&bytes).expect("valid big-endian pcap");
    assert_eq!(file.packets[0].data, &[9, 8, 7, 6]);
}

#[test]
fn rejects_a_truncated_global_header() {
    assert_eq!(parse(&[0u8; 10]), Err(PcapError::TruncatedGlobalHeader));
}

#[test]
fn rejects_a_truncated_frame() {
    let mut bytes = global_header(1, [0xd4, 0xc3, 0xb2, 0xa1], 65535);
    bytes.extend_from_slice(&record(1, 0, 60, 60, &[0u8; 20]));
    let error = parse(&bytes).unwrap_err();
    assert!(error.to_string().contains("truncated frame"), "{error}");
}

#[test]
fn rejects_bad_record_lengths() {
    let mut bytes = global_header(1, [0xd4, 0xc3, 0xb2, 0xa1], 65535);
    bytes.extend_from_slice(&record(1, 0, 70_000, 70_000, &[]));
    let error = parse(&bytes).unwrap_err();
    assert!(error.to_string().contains("bad record lengths"), "{error}");
}

#[test]
fn rejects_an_unknown_link_type() {
    let bytes = global_header(147, [0xd4, 0xc3, 0xb2, 0xa1], 65535);
    assert_eq!(parse(&bytes), Err(PcapError::UnsupportedLinkType(147)));
}

#[test]
fn writes_a_readable_round_trip() {
    let mut bytes = global_header(1, [0xd4, 0xc3, 0xb2, 0xa1], 65535);
    bytes.extend_from_slice(&record(7, 8, 4, 4, &[1, 2, 3, 4]));
    let file = parse(&bytes).expect("valid pcap");
    let written = write(&file.packets, file.link_type, file.snaplen);
    let round_trip = parse(&written).expect("written pcap");
    assert_eq!(round_trip.link_type, 1);
    assert_eq!(round_trip.packets.len(), 1);
    assert_eq!(round_trip.packets[0].data, &[1, 2, 3, 4]);
}

#[test]
fn rejects_a_bad_magic() {
    let bytes = global_header(1, [0, 1, 2, 3], 65535);
    assert!(matches!(parse(&bytes), Err(PcapError::BadMagic(_))));
}
