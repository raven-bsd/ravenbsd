// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the query builder, the answer parser, the question check and EDNS0.

use super::*;
use crate::be16_at;
use alloc::vec::Vec;

const QUERY_IMAGE: [u8; 29] = [
    0x12, 0x34, 0x01, 0x20, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x07, b'e', b'x', b'a',
    b'm', b'p', b'l', b'e', 0x03, b'o', b'r', b'g', 0x00, 0x00, 0x01, 0x00, 0x01,
];

fn query(id: u16) -> Vec<u8> {
    let mut buf = [0u8; 280];
    let len = res_mkquery(0, b"example.org", 1, 1, id, &mut buf).expect("query");
    buf[..len].to_vec()
}

fn answer(name: &[u8], rr_type: u16, rdata: &[u8]) -> Vec<u8> {
    let mut packet = query(0x1234);
    packet[2] |= 0x80;
    packet[3] = 0x80;
    packet[7] = 1;
    packet.push(0xc0);
    packet.push(0x0c);
    packet.extend_from_slice(&rr_type.to_be_bytes());
    packet.extend_from_slice(&1u16.to_be_bytes());
    packet.extend_from_slice(&60u32.to_be_bytes());
    packet.extend_from_slice(&u16::try_from(rdata.len()).unwrap_or(0).to_be_bytes());
    packet.extend_from_slice(rdata);
    let _ = name;
    packet
}

#[test]
fn the_required_query_image() {
    let mut buf = [0u8; 280];
    let len = res_mkquery(0, b"example.org", 1, 1, 0x1234, &mut buf).expect("query");
    assert_eq!(len, 29);
    assert_eq!(&buf[..len], &QUERY_IMAGE);
}

#[test]
fn the_query_opcode_and_ad_bit() {
    let packet = query(0x1234);
    assert_eq!(packet[2], 0x01);
    assert_eq!(packet[3], 0x20);
}

#[test]
fn the_query_counts() {
    let packet = query(0x1234);
    assert_eq!(be16_at(&packet, 4), Some(1));
    assert_eq!(be16_at(&packet, 6), Some(0));
    assert_eq!(be16_at(&packet, 8), Some(0));
    assert_eq!(be16_at(&packet, 10), Some(0));
}

#[test]
fn the_query_id_is_the_caller_id() {
    let packet = query(0xabcd);
    assert_eq!(&packet[..2], &[0xab, 0xcd]);
}

#[test]
fn an_opcode_15_query_sets_the_opcode_bits() {
    let mut buf = [0u8; 280];
    let len = res_mkquery(15, b"a", 1, 1, 1, &mut buf).expect("query");
    assert_eq!(buf[2], 0x79);
    assert!(len > 12);
}

#[test]
fn an_opcode_16_is_rejected() {
    let mut buf = [0u8; 280];
    assert_eq!(res_mkquery(16, b"a", 1, 1, 1, &mut buf), None);
}

#[test]
fn a_class_above_255_is_rejected() {
    let mut buf = [0u8; 280];
    assert_eq!(res_mkquery(0, b"a", 256, 1, 1, &mut buf), None);
}

#[test]
fn a_type_above_255_is_rejected() {
    let mut buf = [0u8; 280];
    assert_eq!(res_mkquery(0, b"a", 1, 256, 1, &mut buf), None);
}

#[test]
fn a_trailing_dot_is_stripped() {
    let mut first = [0u8; 280];
    let mut second = [0u8; 280];
    let a = res_mkquery(0, b"example.org.", 1, 1, 1, &mut first).expect("a");
    let b = res_mkquery(0, b"example.org", 1, 1, 1, &mut second).expect("b");
    assert_eq!(a, b);
    assert_eq!(&first[..a], &second[..b]);
}

#[test]
fn two_trailing_dots_are_rejected() {
    let mut buf = [0u8; 280];
    assert_eq!(res_mkquery(0, b"example.org..", 1, 1, 1, &mut buf), None);
}

#[test]
fn the_root_name_is_seventeen_bytes() {
    let mut buf = [0u8; 280];
    let len = res_mkquery(0, b"", 1, 1, 0x1234, &mut buf).expect("root");
    assert_eq!(len, 17);
    assert_eq!(buf[12], 0);
    assert_eq!(&buf[13..17], &[0, 1, 0, 1]);
}

#[test]
fn a_63_byte_label_is_accepted() {
    let mut name = Vec::new();
    name.extend_from_slice(&[b'a'; 63]);
    let mut buf = [0u8; 280];
    assert!(res_mkquery(0, &name, 1, 1, 1, &mut buf).is_some());
}

#[test]
fn a_64_byte_label_is_rejected() {
    let mut name = Vec::new();
    name.extend_from_slice(&[b'a'; 64]);
    let mut buf = [0u8; 280];
    assert_eq!(res_mkquery(0, &name, 1, 1, 1, &mut buf), None);
}

#[test]
fn a_253_byte_name_is_accepted() {
    let mut name = Vec::new();
    name.extend_from_slice(&[b'a'; 63]);
    name.push(b'.');
    name.extend_from_slice(&[b'b'; 63]);
    name.push(b'.');
    name.extend_from_slice(&[b'c'; 63]);
    name.push(b'.');
    name.extend_from_slice(&[b'd'; 61]);
    let mut buf = [0u8; 280];
    assert!(res_mkquery(0, &name, 1, 1, 1, &mut buf).is_some());
}

#[test]
fn a_254_byte_name_is_rejected() {
    let mut name = Vec::new();
    name.extend_from_slice(&[b'a'; 63]);
    name.push(b'.');
    name.extend_from_slice(&[b'b'; 63]);
    name.push(b'.');
    name.extend_from_slice(&[b'c'; 63]);
    name.push(b'.');
    name.extend_from_slice(&[b'd'; 62]);
    let mut buf = [0u8; 280];
    assert_eq!(res_mkquery(0, &name, 1, 1, 1, &mut buf), None);
}

#[test]
fn a_small_query_buffer_is_rejected() {
    let mut buf = [0u8; 28];
    assert_eq!(res_mkquery(0, b"example.org", 1, 1, 1, &mut buf), None);
}

#[test]
fn pack_edns0_matches_the_required_image() {
    let mut out = [0u8; 16];
    assert_eq!(pack_edns0(1232, false, &mut out), Some(11));
    assert_eq!(
        &out[..11],
        &[0x00, 0x00, 0x29, 0x04, 0xd0, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn pack_edns0_sets_the_do_bit() {
    let mut out = [0u8; 16];
    assert_eq!(pack_edns0(512, true, &mut out), Some(11));
    assert_eq!(out[3], 0x02);
    assert_eq!(out[4], 0x00);
    assert_eq!(out[7], 0x80);
}

#[test]
fn pack_edns0_needs_eleven_bytes() {
    let mut out = [0u8; 10];
    assert_eq!(pack_edns0(1232, false, &mut out), None);
}

#[test]
fn append_edns0_sets_arcount_and_grows_the_query() {
    let mut buf = [0u8; 280];
    let len = res_mkquery(0, b"example.org", 1, 1, 1, &mut buf).expect("query");
    let new_len = append_edns0(&mut buf, len, true).expect("append");
    assert_eq!(new_len, len + 11);
    assert_eq!(be16_at(&buf, 10), Some(1));
    assert_eq!(buf[len + 2], 0x29);
}

#[test]
fn append_edns0_disabled_is_a_no_op() {
    let mut buf = [0u8; 280];
    let len = res_mkquery(0, b"example.org", 1, 1, 1, &mut buf).expect("query");
    assert_eq!(append_edns0(&mut buf, len, false), Some(len));
    assert_eq!(be16_at(&buf, 10), Some(0));
}

#[test]
fn append_edns0_rejects_a_full_buffer() {
    let mut buf = [0u8; 29];
    let len = res_mkquery(0, b"example.org", 1, 1, 1, &mut buf).expect("query");
    assert_eq!(append_edns0(&mut buf, len, true), None);
}

#[test]
fn dns_parse_rejects_a_short_packet() {
    assert_eq!(dns_parse(&[0u8; 11], |_, _, _| 0), -1);
}

#[test]
fn dns_parse_ignores_a_nonzero_rcode() {
    let mut packet = answer(b"example.org", 1, &[1, 2, 3, 4]);
    packet[3] = 3;
    let mut seen = 0;
    assert_eq!(
        dns_parse(&packet, |_, _, _| {
            seen += 1;
            0
        }),
        0
    );
    assert_eq!(seen, 0);
}

#[test]
fn dns_parse_yields_one_a_record() {
    let packet = answer(b"example.org", 1, &[192, 0, 2, 1]);
    let mut rrtype = 0u8;
    let mut rdata = Vec::new();
    assert_eq!(
        dns_parse(&packet, |rr, data, full| {
            rrtype = rr;
            rdata = data.to_vec();
            assert_eq!(full.len(), packet.len());
            0
        }),
        0
    );
    assert_eq!(rrtype, 1);
    assert_eq!(rdata, vec![192, 0, 2, 1]);
}

#[test]
fn dns_parse_yields_an_aaaa_record() {
    let packet = answer(
        b"example.org",
        28,
        &[0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1],
    );
    let mut len = 0usize;
    assert_eq!(
        dns_parse(&packet, |rr, data, _| {
            assert_eq!(rr, 28);
            len = data.len();
            0
        }),
        0
    );
    assert_eq!(len, 16);
}

#[test]
fn dns_parse_returns_minus_one_when_the_callback_rejects() {
    let packet = answer(b"example.org", 1, &[1, 2, 3, 4]);
    assert_eq!(dns_parse(&packet, |_, _, _| -1), -1);
}

#[test]
fn dns_parse_rejects_an_rdata_overrun() {
    let mut packet = answer(b"example.org", 1, &[1, 2, 3, 4]);
    let len = packet.len();
    packet[len - 6] = 0;
    packet[len - 5] = 9;
    assert_eq!(dns_parse(&packet, |_, _, _| 0), -1);
}

#[test]
fn dns_parse_rejects_an_ancount_past_the_end() {
    let mut packet = answer(b"example.org", 1, &[1, 2, 3, 4]);
    packet[7] = 2;
    assert_eq!(dns_parse(&packet, |_, _, _| 0), -1);
}

#[test]
fn dns_parse_rejects_a_qdcount_past_the_end() {
    let mut packet = answer(b"example.org", 1, &[1, 2, 3, 4]);
    packet[5] = 5;
    assert_eq!(dns_parse(&packet, |_, _, _| 0), -1);
}

#[test]
fn dns_parse_rejects_a_bare_query_like_the_donor() {
    // The donor's question loop requires six bytes after the name terminator, which a pure
    // query does not have; resolver callers ignore the return value.
    let packet = query(1);
    assert_eq!(dns_parse(&packet, |_, _, _| 0), -1);
}

#[test]
fn dns_parse_offset_reports_the_rdata_position() {
    let packet = answer(b"example.org", 1, &[1, 2, 3, 4]);
    let mut offset = 0usize;
    assert_eq!(
        dns_parse_offset(&packet, |_, _, _, at| {
            offset = at;
            0
        }),
        0
    );
    assert_eq!(packet[offset], 1);
}

#[test]
fn question_matches_the_query() {
    let request = query(0x1234);
    let response = answer(b"example.org", 1, &[1, 2, 3, 4]);
    assert!(question_matches(&request, &response));
}

#[test]
fn question_matching_is_ascii_case_insensitive() {
    let request = query(0x1234);
    let mut response = answer(b"example.org", 1, &[1, 2, 3, 4]);
    for byte in response.iter_mut().skip(13).take(7) {
        *byte = byte.to_ascii_uppercase();
    }
    assert!(question_matches(&request, &response));
}

#[test]
fn question_matches_rejects_a_different_name() {
    let request = query(0x1234);
    let mut response = answer(b"example.org", 1, &[1, 2, 3, 4]);
    response[13] = b'Q';
    assert!(!question_matches(&request, &response));
}

#[test]
fn question_matches_rejects_a_different_type() {
    let request = query(0x1234);
    let mut response = answer(b"example.org", 1, &[1, 2, 3, 4]);
    response[26] = 28;
    assert!(!question_matches(&request, &response));
}

#[test]
fn question_matches_rejects_a_different_class() {
    let request = query(0x1234);
    let mut response = answer(b"example.org", 1, &[1, 2, 3, 4]);
    response[28] = 2;
    assert!(!question_matches(&request, &response));
}

#[test]
fn question_matches_requires_qr() {
    let request = query(0x1234);
    let response = query(0x1234);
    assert!(!question_matches(&request, &response));
}

#[test]
fn question_matches_requires_a_clear_qr_on_the_query() {
    let mut request = query(0x1234);
    request[2] |= 0x80;
    let response = answer(b"example.org", 1, &[1, 2, 3, 4]);
    assert!(!question_matches(&request, &response));
}

#[test]
fn question_matches_requires_qdcount_one() {
    let request = query(0x1234);
    let mut zero = answer(b"example.org", 1, &[1, 2, 3, 4]);
    zero[5] = 0;
    assert!(!question_matches(&request, &zero));
    let mut two = answer(b"example.org", 1, &[1, 2, 3, 4]);
    two[5] = 2;
    assert!(!question_matches(&request, &two));
}

#[test]
fn question_matches_requires_opcode_zero() {
    let request = query(0x1234);
    let mut response = answer(b"example.org", 1, &[1, 2, 3, 4]);
    response[2] |= 0x08;
    assert!(!question_matches(&request, &response));
}

#[test]
fn question_matches_rejects_short_packets() {
    let request = query(0x1234);
    assert!(!question_matches(&request, &[0u8; 11]));
    assert!(!question_matches(
        &[0u8; 11],
        &answer(b"example.org", 1, &[1, 2, 3, 4])
    ));
}
