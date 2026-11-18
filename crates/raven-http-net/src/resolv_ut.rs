// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the fixed `resolv:` query/reply codec.

use super::{MAX_NAME, QTYPE_A, STATUS_OK, VERSION, decode_reply, encode_query};
use crate::{ResolveError, Resolved};
use alloc::vec;
use core::net::Ipv4Addr;

fn reply(status: u8, records: &[[u8; 4]]) -> vec::Vec<u8> {
    let mut out = vec::Vec::new();
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.push(status);
    out.push(QTYPE_A);
    out.push(records.len() as u8);
    for address in records {
        out.push(QTYPE_A);
        out.extend_from_slice(&60u32.to_le_bytes());
        out.extend_from_slice(address);
    }
    out
}

#[test]
fn encodes_an_a_query() {
    let mut buffer = [0u8; 256];
    let length = encode_query("example.com", &mut buffer).expect("query");
    assert_eq!(length, 16);
    assert_eq!(&buffer[..2], &VERSION.to_le_bytes());
    assert_eq!(buffer[2], QTYPE_A);
    assert_eq!(&buffer[3..5], &11u16.to_le_bytes());
    assert_eq!(&buffer[5..16], b"example.com");
}

#[test]
fn rejects_bad_names_and_short_buffers() {
    let mut buffer = [0u8; 256];
    for name in ["", "a..b", ".a", "a.", "bad space"] {
        assert_eq!(
            encode_query(name, &mut buffer),
            Err(ResolveError::NotFound),
            "{name}"
        );
    }
    let long = "a".repeat(MAX_NAME + 1);
    assert_eq!(
        encode_query(&long, &mut buffer),
        Err(ResolveError::NotFound)
    );
    let mut tiny = [0u8; 4];
    assert_eq!(
        encode_query("example.com", &mut tiny),
        Err(ResolveError::Io)
    );
}

#[test]
fn decodes_the_first_a_record() {
    let wire = reply(STATUS_OK, &[[192, 0, 2, 10], [192, 0, 2, 11]]);
    assert_eq!(
        decode_reply(&wire),
        Ok(Resolved {
            address: Ipv4Addr::new(192, 0, 2, 10),
            ttl_secs: 60,
        })
    );
}

#[test]
fn maps_nxdomain_and_server_errors_to_not_found() {
    assert_eq!(decode_reply(&reply(1, &[])), Err(ResolveError::NotFound));
    assert_eq!(decode_reply(&reply(2, &[])), Err(ResolveError::NotFound));
    assert_eq!(
        decode_reply(&reply(STATUS_OK, &[])),
        Err(ResolveError::NotFound)
    );
}

#[test]
fn rejects_bad_headers_and_lengths() {
    let mut wire = reply(STATUS_OK, &[[192, 0, 2, 10]]);
    wire[0] = 2;
    assert_eq!(decode_reply(&wire), Err(ResolveError::Io));

    let mut wire = reply(STATUS_OK, &[[192, 0, 2, 10]]);
    wire[3] = 28;
    assert_eq!(decode_reply(&wire), Err(ResolveError::Io));

    let mut wire = reply(STATUS_OK, &[[192, 0, 2, 10]]);
    wire.push(0);
    assert_eq!(decode_reply(&wire), Err(ResolveError::NotFound));

    let mut too_many = reply(STATUS_OK, &[]);
    too_many[4] = 9;
    too_many.extend_from_slice(&[0; 9 * 9]);
    assert_eq!(decode_reply(&too_many), Err(ResolveError::NotFound));

    assert_eq!(decode_reply(&[1, 0, 0]), Err(ResolveError::Io));
}
