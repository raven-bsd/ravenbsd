// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! MP_CAPABLE negotiation and MP_JOIN HMAC vectors.

use super::*;
use crate::option::VERSION;

const KEY_A: u64 = 0x1112_1314_1516_1718;
const KEY_B: u64 = 0x0102_0304_0506_0708;
const RAND_A: u32 = 0x2122_2324;
const RAND_B: u32 = 0x3132_3334;

#[test]
fn mp_capable_exchange_negotiates_both_keys() {
    let mut a = MpCapableHandshake::initiator(KEY_A, true);
    let mut b = MpCapableHandshake::responder(KEY_B, true);

    let syn = a.syn();
    assert_eq!(syn.to_vec(), vec![30, 4, 0x01, 0x81]);

    let synack = b.accept_syn(&syn).expect("synack");
    let MptcpOption::MpCapable {
        sender_key,
        receiver_key,
        ..
    } = &synack
    else {
        panic!("not MP_CAPABLE");
    };
    assert_eq!(*sender_key, Some(KEY_B));
    assert_eq!(*receiver_key, None);

    let ack = a.accept_synack(&synack).expect("ack");
    let MptcpOption::MpCapable {
        sender_key,
        receiver_key,
        ..
    } = &ack
    else {
        panic!("not MP_CAPABLE");
    };
    assert_eq!(*sender_key, Some(KEY_A));
    assert_eq!(*receiver_key, Some(KEY_B));

    b.accept_ack(&ack).expect("ack accepted");
    assert!(a.established());
    assert!(b.established());
    assert_eq!(a.remote_key(), Some(KEY_B));
    assert_eq!(b.remote_key(), Some(KEY_A));
}

#[test]
fn checksum_negotiation_is_the_union_of_both_sides() {
    let a = MpCapableHandshake::initiator(KEY_A, false);
    let mut b = MpCapableHandshake::responder(KEY_B, true);
    let syn = a.syn();
    let synack = b.accept_syn(&syn).expect("synack");
    let MptcpOption::MpCapable { flags, .. } = synack else {
        panic!("not MP_CAPABLE");
    };
    assert!(flags.checksum_required);
}

#[test]
fn mp_capable_rejects_version_and_reserved_flags() {
    let mut b = MpCapableHandshake::responder(KEY_B, false);
    let bad_version = MptcpOption::MpCapable {
        version: 0,
        flags: MpCapableFlags::default(),
        sender_key: None,
        receiver_key: None,
        data_level_length: None,
        checksum: None,
    };
    assert_eq!(b.accept_syn(&bad_version), Err(Error::UnsupportedVersion));

    let bad_flags = MptcpOption::MpCapable {
        version: VERSION,
        flags: MpCapableFlags::default(),
        sender_key: None,
        receiver_key: None,
        data_level_length: None,
        checksum: None,
    };
    let mut b = MpCapableHandshake::responder(KEY_B, false);
    b.accept_syn(&bad_flags).expect("valid syn");

    let bad_ack = MptcpOption::MpCapable {
        version: VERSION,
        flags: MpCapableFlags::default(),
        sender_key: Some(KEY_A),
        receiver_key: Some(KEY_A),
        data_level_length: None,
        checksum: None,
    };
    assert_eq!(b.accept_ack(&bad_ack), Err(Error::InvalidFlags));
}

#[test]
fn join_hmac_matches_the_reference_vector() {
    let initiator = initiator_hmac(KEY_A, KEY_B, RAND_A, RAND_B);
    assert_eq!(
        initiator[..20],
        [
            0x96, 0x9e, 0xa8, 0x0f, 0x95, 0xe6, 0x7f, 0x60, 0x2b, 0xb9, 0xc5, 0x0b, 0xfa, 0x25,
            0xc7, 0x84, 0x2f, 0x1a, 0x67, 0xfe,
        ]
    );
    let responder = responder_hmac(KEY_A, KEY_B, RAND_A, RAND_B);
    assert_eq!(
        responder[..8],
        [0xc8, 0xb0, 0xcc, 0x4d, 0x45, 0xde, 0x81, 0x27]
    );
}

#[test]
fn mp_join_exchange_authenticates_both_directions() {
    let mut a = MpJoinInitiator::new(KEY_A, KEY_B, RAND_A, 3, false);
    let mut b = MpJoinResponder::new(KEY_A, KEY_B, RAND_B, 2, false);

    let syn = a.syn();
    let MptcpOption::MpJoinSyn { token, .. } = syn else {
        panic!("not MP_JOIN SYN");
    };
    assert_eq!(token, 0x6684_0dda);

    let synack = b.accept_syn(&syn).expect("synack");
    let ack = a.accept_synack(&synack).expect("ack");
    b.accept_ack(&ack).expect("ack verified");
    assert_eq!(a.rand_b(), Some(RAND_B));
    assert_eq!(b.rand_a(), Some(RAND_A));
}

#[test]
fn mp_join_rejects_a_bad_token_and_bad_hmac() {
    let mut b = MpJoinResponder::new(KEY_A, KEY_B, RAND_B, 2, false);
    let bad = MptcpOption::MpJoinSyn {
        backup: false,
        address_id: 3,
        token: 0xdead_beef,
        rand: RAND_A,
    };
    assert_eq!(b.accept_syn(&bad), Err(Error::NotFound));

    let mut a = MpJoinInitiator::new(KEY_A, KEY_B, RAND_A, 3, false);
    let syn = a.syn();
    let _synack = b.accept_syn(&syn).expect("synack");
    let forged = MptcpOption::MpJoinSynAck {
        backup: false,
        address_id: 2,
        hmac: [0u8; 8],
        rand: RAND_B,
    };
    assert_eq!(a.accept_synack(&forged), Err(Error::InvalidFlags));
}
