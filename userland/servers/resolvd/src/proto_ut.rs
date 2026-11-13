// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! `resolv:` protocol tests.

use alloc::vec;

use super::*;

#[test]
fn query_round_trips() {
    let query = Query {
        qtype: QTYPE_A,
        name: "a.test",
    };
    let mut bytes = [0u8; 32];
    let length = query.encode(&mut bytes).expect("encode");
    assert_eq!(length, 11);
    assert_eq!(Query::decode(&bytes[..length]), Ok(query));
}

#[test]
fn query_rejects_bad_fields() {
    let query = Query {
        qtype: QTYPE_A,
        name: "a.test",
    };
    let mut bytes = [0u8; 32];
    let length = query.encode(&mut bytes).expect("encode");

    let mut bad = bytes;
    bad[0] = 2;
    assert_eq!(Query::decode(&bad[..length]), Err(ProtoError::BadVersion));

    let mut bad = bytes;
    bad[2] = 99;
    assert_eq!(Query::decode(&bad[..length]), Err(ProtoError::BadType));

    let mut bad = bytes;
    bad[3..5].copy_from_slice(&1000u16.to_le_bytes());
    assert_eq!(Query::decode(&bad[..length]), Err(ProtoError::BadLength));

    let mut trailing = bytes.to_vec();
    trailing.push(0);
    assert_eq!(
        Query::decode(&trailing[..length + 1]),
        Err(ProtoError::TrailingBytes)
    );
    assert_eq!(Query::decode(&bytes[..4]), Err(ProtoError::TooShort));
}

#[test]
fn names_are_validated() {
    assert!(validate_name("a.test").is_ok());
    assert!(validate_name("a.test.").is_ok());
    assert!(validate_name("_service._tcp.test").is_ok());
    assert_eq!(validate_name(""), Err(ProtoError::BadName));
    assert_eq!(validate_name(".test"), Err(ProtoError::BadName));
    assert_eq!(validate_name("a..test"), Err(ProtoError::BadName));
    assert_eq!(validate_name("a/b"), Err(ProtoError::BadName));
}

#[test]
fn a_reply_round_trips() {
    let reply = Reply::answers(
        QTYPE_A,
        vec![Record {
            ttl: 60,
            address: Address::V4([192, 0, 2, 10]),
        }],
    );
    let mut bytes = [0u8; 64];
    let length = reply.encode(&mut bytes).expect("encode");
    assert_eq!(length, 14);
    assert_eq!(Reply::decode(&bytes[..length]), Ok(reply));
}

#[test]
fn aaaa_and_nxdomain_round_trip() {
    let reply = Reply::answers(
        QTYPE_AAAA,
        vec![Record {
            ttl: 300,
            address: Address::V6([0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
        }],
    );
    let mut bytes = [0u8; 64];
    let length = reply.encode(&mut bytes).expect("encode");
    assert_eq!(length, 26);
    assert_eq!(Reply::decode(&bytes[..length]), Ok(reply));

    let reply = Reply::nxdomain(QTYPE_A);
    let length = reply.encode(&mut bytes).expect("encode");
    assert_eq!(Reply::decode(&bytes[..length]), Ok(reply));
}

#[test]
fn reply_rejects_mismatched_address_width() {
    let reply = Reply::answers(
        QTYPE_A,
        vec![Record {
            ttl: 60,
            address: Address::V6([0; 16]),
        }],
    );
    let mut bytes = [0u8; 64];
    assert_eq!(reply.encode(&mut bytes), Err(ProtoError::BadType));
}
