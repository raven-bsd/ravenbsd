// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Byte-exact RFC 8684 option vectors at the public crate API.

use raven_mptcp::{
    Address, Error, MpCapableFlags, MpCapableHandshake, MpJoinInitiator, MpJoinResponder,
    MptcpOption, Path, PathManager, RemoveIds, TcpRstReason,
};

fn hex(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    digits
        .chunks_exact(2)
        .map(|pair| {
            let high = (pair[0] as char).to_digit(16).expect("high");
            let low = (pair[1] as char).to_digit(16).expect("low");
            ((high << 4) | low) as u8
        })
        .collect()
}

const VECTORS: &[(&str, &str)] = &[
    ("mp_capable_syn", "1e 04 01 01"),
    ("mp_capable_synack", "1e 0c 01 01 01 02 03 04 05 06 07 08"),
    (
        "mp_capable_ack",
        "1e 14 01 01 11 12 13 14 15 16 17 18 01 02 03 04 05 06 07 08",
    ),
    (
        "mp_capable_ack_data",
        "1e 16 01 01 11 12 13 14 15 16 17 18 01 02 03 04 05 06 07 08 00 2c",
    ),
    ("mp_join_syn", "1e 0c 10 03 11 22 33 44 55 66 77 88"),
    (
        "mp_join_synack",
        "1e 10 10 02 00 01 02 03 04 05 06 07 99 aa bb cc",
    ),
    (
        "mp_join_ack",
        "1e 18 10 00 00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f 10 11 12 13",
    ),
    ("dss_ack", "1e 08 20 01 01 02 03 04"),
    (
        "dss_full",
        "1e 1c 20 1f 01 02 03 04 05 06 07 08 11 12 13 14 15 16 17 18 21 22 23 24 00 2c 33 44",
    ),
    (
        "add_addr_v4",
        "1e 10 30 05 c0 00 02 01 a1 a2 a3 a4 a5 a6 a7 a8",
    ),
    ("add_addr_v4_echo", "1e 08 31 05 c0 00 02 01"),
    (
        "add_addr_v6",
        "1e 1c 30 06 20 01 0d b8 00 00 00 00 00 00 00 00 00 00 00 01 a1 a2 a3 a4 a5 a6 a7 a8",
    ),
    ("remove_addr", "1e 06 40 03 07 09"),
    ("mp_prio", "1e 03 50"),
    ("mp_fail", "1e 0c 60 00 01 02 03 04 05 06 07 08"),
    ("mp_fastclose", "1e 0c 70 00 11 12 13 14 15 16 17 18"),
    ("mp_tcp_rst", "1e 04 80 01"),
];

#[test]
fn every_vector_round_trips_byte_for_byte() {
    for (name, text) in VECTORS {
        let bytes = hex(text);
        let (option, used) = MptcpOption::parse(&bytes).unwrap_or_else(|error| {
            panic!("{name}: parse failed: {error:?}");
        });
        assert_eq!(used, bytes.len(), "{name}: used length");
        assert_eq!(option.to_vec(), bytes, "{name}: re-encode");
    }
}

#[test]
fn typed_vectors_have_the_rfc_fields() {
    let (option, _) = MptcpOption::parse(&hex("1e 0c 10 03 11 22 33 44 55 66 77 88")).expect("syn");
    assert_eq!(
        option,
        MptcpOption::MpJoinSyn {
            backup: false,
            address_id: 3,
            token: 0x1122_3344,
            rand: 0x5566_7788,
        }
    );

    let (option, _) =
        MptcpOption::parse(&hex("1e 10 30 05 c0 00 02 01 a1 a2 a3 a4 a5 a6 a7 a8")).expect("add");
    assert_eq!(
        option,
        MptcpOption::AddAddr {
            echo: false,
            address_id: 5,
            address: Address::V4([192, 0, 2, 1]),
            port: None,
            hmac: Some([0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8]),
        }
    );

    let (option, _) = MptcpOption::parse(&hex("1e 04 81 02")).expect("rst");
    assert_eq!(
        option,
        MptcpOption::MpTcpRst {
            transient: true,
            reason: TcpRstReason::LackOfResources,
        }
    );
}

#[test]
fn every_truncated_prefix_fails_closed() {
    for (name, text) in VECTORS {
        let bytes = hex(text);
        for prefix in 0..bytes.len() {
            assert!(
                MptcpOption::parse(&bytes[..prefix]).is_err(),
                "{name}: prefix {prefix} accepted"
            );
        }
    }
}

#[test]
fn malformed_requests_return_errors_not_panics() {
    assert_eq!(MptcpOption::parse(&[]), Err(Error::Truncated));
    assert_eq!(
        MptcpOption::parse(&[30, 3, 0x90]),
        Err(Error::UnknownSubtype)
    );
    assert_eq!(
        MptcpOption::parse(&[30, 4, 0x00, 0]),
        Err(Error::UnsupportedVersion)
    );
    assert_eq!(
        MptcpOption::parse(&hex("1e 03 40 03")),
        Err(Error::BadLength)
    );
    assert!(RemoveIds::new(vec![]).is_err());
}

#[test]
fn option_list_handles_padding_and_foreign_options() {
    let mut bytes = vec![1, 2, 4, 0x05, 0xb4];
    bytes.extend_from_slice(&hex("1e 04 01 01"));
    bytes.push(1);
    bytes.extend_from_slice(&hex("1e 03 51"));
    bytes.push(0);
    let options = raven_mptcp::option::parse_options(&bytes).expect("list");
    assert_eq!(options.len(), 2);
    assert!(matches!(options[0], MptcpOption::MpCapable { .. }));
    assert_eq!(options[1], MptcpOption::MpPrio { backup: true });
}

#[test]
fn mp_capable_vector_drives_the_state_machine() {
    let mut responder = MpCapableHandshake::responder(0x0102_0304_0506_0708, false);
    let (syn, _) = MptcpOption::parse(&hex("1e 04 01 01")).expect("syn");
    let synack = responder.accept_syn(&syn).expect("synack");
    assert_eq!(synack.to_vec(), hex("1e 0c 01 01 01 02 03 04 05 06 07 08"));

    let mut initiator = MpCapableHandshake::initiator(0x1112_1314_1516_1718, false);
    let (synack, _) = MptcpOption::parse(&synack.to_vec()).expect("synack");
    let ack = initiator.accept_synack(&synack).expect("ack");
    let (ack, _) = MptcpOption::parse(&ack.to_vec()).expect("ack");
    responder.accept_ack(&ack).expect("ack verified");
}

#[test]
fn mp_join_vector_authenticates() {
    let mut initiator = MpJoinInitiator::new(
        0x1112_1314_1516_1718,
        0x0102_0304_0506_0708,
        0x2122_2324,
        3,
        false,
    );
    let mut responder = MpJoinResponder::new(
        0x1112_1314_1516_1718,
        0x0102_0304_0506_0708,
        0x3132_3334,
        2,
        false,
    );
    let syn = initiator.syn();
    let synack = responder.accept_syn(&syn).expect("synack");
    let ack = initiator.accept_synack(&synack).expect("ack");
    responder.accept_ack(&ack).expect("ack verified");
    assert!(matches!(ack, MptcpOption::MpJoinAck { .. }));
}

#[test]
fn path_manager_matches_the_signalling() {
    let mut manager = PathManager::new();
    manager
        .announce(Path::new(1, Address::V4([192, 0, 2, 10]), None))
        .expect("path 1");
    manager
        .announce(Path::new(2, Address::V6([0; 16]), Some(443)))
        .expect("path 2");
    manager.set_backup(2, true).expect("backup");
    assert_eq!(manager.select().map(|path| path.address_id), Some(1));
    manager.remove_ids(&[1]);
    assert_eq!(manager.select().map(|path| path.address_id), Some(2));
    assert_eq!(manager.remove(9), Err(Error::NotFound));
}

#[test]
fn mp_capable_flags_reject_reserved_bits() {
    assert_eq!(MpCapableFlags::from_byte(0x00), Err(Error::InvalidFlags));
    assert_eq!(MpCapableFlags::from_byte(0x41), Err(Error::InvalidFlags));
    assert!(
        MpCapableFlags::from_byte(0x81)
            .expect("valid")
            .checksum_required
    );
}
