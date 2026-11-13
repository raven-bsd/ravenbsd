// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! DNS codec tests.

use alloc::vec::Vec;

use super::*;

fn put_name(out: &mut Vec<u8>, name: &str) {
    for label in name.split('.') {
        out.push(label.len() as u8);
        out.extend_from_slice(label.as_bytes());
    }
    out.push(0);
}

fn response(flags: u16, answers: &[(u16, u32, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&0x1234u16.to_be_bytes());
    out.extend_from_slice(&flags.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&(answers.len() as u16).to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    put_name(&mut out, "a.test");
    out.extend_from_slice(&TYPE_A.to_be_bytes());
    out.extend_from_slice(&CLASS_IN.to_be_bytes());
    for (kind, ttl, data) in answers {
        out.extend_from_slice(&[0xc0, 0x0c]);
        out.extend_from_slice(&kind.to_be_bytes());
        out.extend_from_slice(&CLASS_IN.to_be_bytes());
        out.extend_from_slice(&ttl.to_be_bytes());
        out.extend_from_slice(&(data.len() as u16).to_be_bytes());
        out.extend_from_slice(data);
    }
    out
}

#[test]
fn query_has_the_standard_header_and_question() {
    let query = build_query(0x1234, "a.test", TYPE_A).expect("query");
    assert_eq!(
        query,
        [
            0x12, 0x34, 0x01, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, b'a',
            0x04, b't', b'e', b's', b't', 0x00, 0x00, 0x01, 0x00, 0x01,
        ]
    );
}

#[test]
fn a_and_aaaa_answers_decode() {
    let a = response(0x8180, &[(TYPE_A, 60, &[192, 0, 2, 10])]);
    let parsed = parse_response(&a, 0x1234).expect("A");
    assert_eq!(parsed.rcode, 0);
    assert_eq!(parsed.address(), Some((60, Address::V4([192, 0, 2, 10]))));

    let v6 = [0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
    let aaaa = response(0x8180, &[(TYPE_AAAA, 300, &v6)]);
    let parsed = parse_response(&aaaa, 0x1234).expect("AAAA");
    assert_eq!(parsed.address(), Some((300, Address::V6(v6))));
}

#[test]
fn cname_and_nxdomain_decode() {
    let mut target = Vec::new();
    put_name(&mut target, "target.test");
    let cname = response(0x8180, &[(TYPE_CNAME, 60, &target)]);
    let parsed = parse_response(&cname, 0x1234).expect("CNAME");
    assert_eq!(parsed.cname(), Some("target.test"));

    let nx = response(0x8183, &[]);
    let parsed = parse_response(&nx, 0x1234).expect("NXDOMAIN");
    assert_eq!(parsed.rcode, 3);
    assert!(parsed.answers.is_empty());
}

#[test]
fn malformed_packets_are_rejected() {
    let a = response(0x8180, &[(TYPE_A, 60, &[192, 0, 2, 10])]);
    assert_eq!(parse_response(&a, 0x9999), Err(DnsError::BadHeader));

    let mut not_response = a.clone();
    not_response[2] &= 0x7f;
    assert_eq!(
        parse_response(&not_response, 0x1234),
        Err(DnsError::BadHeader)
    );

    assert_eq!(parse_response(&a[..8], 0x1234), Err(DnsError::TooShort));
    assert_eq!(
        parse_response(&a[..a.len() - 1], 0x1234),
        Err(DnsError::TooShort)
    );

    let mut bad_rcode = a;
    bad_rcode[3] = 2;
    assert_eq!(
        parse_response(&bad_rcode, 0x1234),
        Err(DnsError::ServerError(2))
    );
}

#[test]
fn compression_pointers_are_bounded() {
    let mut packet = Vec::new();
    packet.extend_from_slice(&0x1234u16.to_be_bytes());
    packet.extend_from_slice(&0x8180u16.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.extend_from_slice(&1u16.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    packet.extend_from_slice(&0u16.to_be_bytes());
    // The answer name points at itself.
    packet.extend_from_slice(&[0xc0, 0x0c]);
    packet.extend_from_slice(&TYPE_A.to_be_bytes());
    packet.extend_from_slice(&CLASS_IN.to_be_bytes());
    packet.extend_from_slice(&60u32.to_be_bytes());
    packet.extend_from_slice(&4u16.to_be_bytes());
    packet.extend_from_slice(&[192, 0, 2, 1]);
    assert_eq!(parse_response(&packet, 0x1234), Err(DnsError::BadName));
}

#[test]
fn invalid_query_names_are_rejected() {
    assert_eq!(build_query(1, "", TYPE_A), Err(DnsError::BadName));
    assert_eq!(build_query(1, "a..test", TYPE_A), Err(DnsError::BadName));
    assert_eq!(
        build_query(1, "a.test", TYPE_CNAME),
        Err(DnsError::BadRecord)
    );
}
