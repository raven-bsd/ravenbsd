// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the nameser parser.

use super::*;
use alloc::vec::Vec;

fn message() -> Vec<u8> {
    let mut buf = [0u8; 280];
    let len = crate::res_mkquery(0, b"example.org", 1, 1, 0x1234, &mut buf).expect("query");
    let mut packet = buf[..len].to_vec();
    packet[2] |= 0x80;
    packet[3] = 0x80;
    packet[7] = 1;
    packet.extend_from_slice(&[0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01]);
    packet.extend_from_slice(&60u32.to_be_bytes());
    packet.extend_from_slice(&4u16.to_be_bytes());
    packet.extend_from_slice(&[192, 0, 2, 1]);
    packet
}

#[test]
fn get16_reads_big_endian() {
    assert_eq!(ns_get16(&[0x12, 0x34]), Some(0x1234));
    assert_eq!(ns_get16(&[0x12]), None);
}

#[test]
fn get32_reads_big_endian() {
    assert_eq!(ns_get32(&[0x12, 0x34, 0x56, 0x78]), Some(0x1234_5678));
    assert_eq!(ns_get32(&[0x12, 0x34, 0x56]), None);
}

#[test]
fn put16_writes_big_endian() {
    let mut buf = [0u8; 4];
    assert!(ns_put16(0x1234, &mut buf));
    assert_eq!(&buf[..2], &[0x12, 0x34]);
    assert!(!ns_put16(1, &mut [0u8; 1]));
}

#[test]
fn put32_writes_big_endian() {
    let mut buf = [0u8; 4];
    assert!(ns_put32(0x1234_5678, &mut buf));
    assert_eq!(buf, [0x12, 0x34, 0x56, 0x78]);
    assert!(!ns_put32(1, &mut [0u8; 3]));
}

#[test]
fn flagdata_matches_the_donor_table() {
    assert_eq!(NS_FLAGDATA[0], (0x8000, 15));
    assert_eq!(NS_FLAGDATA[1], (0x7800, 11));
    assert_eq!(NS_FLAGDATA[2], (0x0400, 10));
    assert_eq!(NS_FLAGDATA[9], (0x000f, 0));
    assert_eq!(NS_FLAGDATA[15], (0x0000, 0));
}

#[test]
fn initparse_reads_the_header() {
    let packet = message();
    let handle = ns_initparse(&packet).expect("parse");
    assert_eq!(handle.id(), 0x1234);
    assert_eq!(handle.flags(), 0x8180);
    assert_eq!(handle.count(NS_S_QD), Some(1));
    assert_eq!(handle.count(NS_S_AN), Some(1));
    assert_eq!(handle.count(NS_S_NS), Some(0));
    assert_eq!(handle.count(NS_S_AR), Some(0));
}

#[test]
fn initparse_rejects_a_short_packet() {
    assert!(ns_initparse(&[0u8; 11]).is_none());
}

#[test]
fn initparse_rejects_trailing_bytes() {
    let mut packet = message();
    packet.push(0);
    assert!(ns_initparse(&packet).is_none());
}

#[test]
fn initparse_rejects_a_bad_section_count() {
    let mut packet = message();
    packet[7] = 2;
    assert!(ns_initparse(&packet).is_none());
}

#[test]
fn initparse_accepts_an_empty_message() {
    let mut packet = message();
    packet[5] = 0;
    packet[7] = 0;
    packet.truncate(12);
    let handle = ns_initparse(&packet).expect("parse");
    assert_eq!(handle.count(NS_S_QD), Some(0));
}

#[test]
fn skiprr_counts_the_question() {
    let packet = message();
    assert_eq!(ns_skiprr(&packet, 12, packet.len(), NS_S_QD, 1), Some(17));
}

#[test]
fn skiprr_counts_the_answer() {
    let packet = message();
    assert_eq!(ns_skiprr(&packet, 29, packet.len(), NS_S_AN, 1), Some(16));
}

#[test]
fn skiprr_rejects_a_negative_count() {
    let packet = message();
    assert_eq!(ns_skiprr(&packet, 12, packet.len(), NS_S_QD, -1), None);
}

#[test]
fn skiprr_rejects_a_truncated_record() {
    let packet = message();
    assert_eq!(ns_skiprr(&packet, 29, 32, NS_S_AN, 1), None);
}

#[test]
fn parserr_reads_the_question() {
    let packet = message();
    let mut handle = ns_initparse(&packet).expect("parse");
    let rr = ns_parserr(&mut handle, NS_S_QD, 0).expect("rr");
    assert_eq!(rr.name, "example.org");
    assert_eq!(rr.rr_type, 1);
    assert_eq!(rr.rr_class, 1);
    assert_eq!(rr.ttl, 0);
    assert_eq!(rr.rdata, &[]);
}

#[test]
fn parserr_reads_the_answer() {
    let packet = message();
    let mut handle = ns_initparse(&packet).expect("parse");
    let rr = ns_parserr(&mut handle, NS_S_AN, 0).expect("rr");
    assert_eq!(rr.name, "example.org");
    assert_eq!(rr.rr_type, 1);
    assert_eq!(rr.rr_class, 1);
    assert_eq!(rr.ttl, 60);
    assert_eq!(rr.rdata, &[192, 0, 2, 1]);
}

#[test]
fn parserr_with_minus_one_reads_sequentially() {
    let mut packet = message();
    packet[7] = 2;
    let answer = [
        0xc0, 0x0c, 0x00, 0x01, 0x00, 0x01, 0, 0, 0, 60, 0, 4, 192, 0, 2, 2,
    ];
    packet.extend_from_slice(&answer);
    let mut handle = ns_initparse(&packet).expect("parse");
    let first = ns_parserr(&mut handle, NS_S_AN, -1).expect("first");
    assert_eq!(first.rdata, &[192, 0, 2, 1]);
    let second = ns_parserr(&mut handle, NS_S_AN, -1).expect("second");
    assert_eq!(second.rdata, &[192, 0, 2, 2]);
}

#[test]
fn parserr_rewinds_for_an_earlier_number() {
    let packet = message();
    let mut handle = ns_initparse(&packet).expect("parse");
    assert!(ns_parserr(&mut handle, NS_S_AN, 0).is_some());
    assert!(ns_parserr(&mut handle, NS_S_QD, 0).is_some());
    assert!(ns_parserr(&mut handle, NS_S_AN, 0).is_some());
}

#[test]
fn parserr_rejects_a_bad_section() {
    let packet = message();
    let mut handle = ns_initparse(&packet).expect("parse");
    assert!(ns_parserr(&mut handle, -1, 0).is_none());
    assert!(ns_parserr(&mut handle, NS_S_MAX, 0).is_none());
}

#[test]
fn parserr_rejects_a_bad_record_number() {
    let packet = message();
    let mut handle = ns_initparse(&packet).expect("parse");
    assert!(ns_parserr(&mut handle, NS_S_AN, 5).is_none());
    assert!(ns_parserr(&mut handle, NS_S_AN, -2).is_none());
}

#[test]
fn name_uncompress_reads_the_pointer() {
    let packet = message();
    let mut out = [0u8; 256];
    let (consumed, len) = ns_name_uncompress(&packet, packet.len(), 29, &mut out).expect("name");
    assert_eq!(consumed, 2);
    assert_eq!(&out[..len], b"example.org");
}

#[test]
fn name_uncompress_rejects_a_bad_end() {
    let packet = message();
    let mut out = [0u8; 256];
    assert!(ns_name_uncompress(&packet, packet.len() + 1, 29, &mut out).is_none());
}

#[test]
fn initparse_rejects_an_rdata_overrun() {
    let mut packet = message();
    let len = packet.len();
    packet[len - 6] = 0;
    packet[len - 5] = 9;
    assert!(ns_initparse(&packet).is_none());
}
