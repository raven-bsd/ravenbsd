// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Byte-exact option vectors and fail-closed decoder tests.

use super::*;
use crate::handshake::MpCapableFlags;

fn hex(text: &str) -> Vec<u8> {
    let digits: Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    let mut out = Vec::new();
    let mut index = 0;
    while index + 1 < digits.len() {
        let high = (digits[index] as char).to_digit(16).expect("high");
        let low = (digits[index + 1] as char).to_digit(16).expect("low");
        out.push(((high << 4) | low) as u8);
        index += 2;
    }
    out
}

fn capable(
    sender: Option<u64>,
    receiver: Option<u64>,
    data: Option<(u16, Option<u16>)>,
) -> MptcpOption {
    MptcpOption::MpCapable {
        version: VERSION,
        flags: MpCapableFlags::default(),
        sender_key: sender,
        receiver_key: receiver,
        data_level_length: data.map(|value| value.0),
        checksum: data.and_then(|value| value.1),
    }
}

fn add(
    echo: bool,
    id: u8,
    address: Address,
    port: Option<u16>,
    hmac: Option<[u8; 8]>,
) -> MptcpOption {
    MptcpOption::AddAddr {
        echo,
        address_id: id,
        address,
        port,
        hmac,
    }
}

#[test]
fn handshake_options_encode_byte_exact() {
    let key_b = 0x0102_0304_0506_0708u64;
    let key_a = 0x1112_1314_1516_1718u64;
    let vectors = [
        (capable(None, None, None), "1e 04 01 01"),
        (
            capable(Some(key_b), None, None),
            "1e 0c 01 01 01 02 03 04 05 06 07 08",
        ),
        (
            capable(Some(key_a), Some(key_b), None),
            "1e 14 01 01 11 12 13 14 15 16 17 18 01 02 03 04 05 06 07 08",
        ),
        (
            capable(Some(key_a), Some(key_b), Some((44, None))),
            "1e 16 01 01 11 12 13 14 15 16 17 18 01 02 03 04 05 06 07 08 00 2c",
        ),
        (
            capable(Some(key_a), Some(key_b), Some((44, Some(0x9b53)))),
            "1e 18 01 01 11 12 13 14 15 16 17 18 01 02 03 04 05 06 07 08 00 2c 9b 53",
        ),
        (
            MptcpOption::MpJoinSyn {
                backup: false,
                address_id: 3,
                token: 0x1122_3344,
                rand: 0x5566_7788,
            },
            "1e 0c 10 03 11 22 33 44 55 66 77 88",
        ),
        (
            MptcpOption::MpJoinSyn {
                backup: true,
                address_id: 3,
                token: 0x1122_3344,
                rand: 0x5566_7788,
            },
            "1e 0c 11 03 11 22 33 44 55 66 77 88",
        ),
        (
            MptcpOption::MpJoinSynAck {
                backup: false,
                address_id: 2,
                hmac: [0, 1, 2, 3, 4, 5, 6, 7],
                rand: 0x99aa_bbcc,
            },
            "1e 10 10 02 00 01 02 03 04 05 06 07 99 aa bb cc",
        ),
        (
            MptcpOption::MpJoinAck {
                hmac: [
                    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
                ],
            },
            "1e 18 10 00 00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f 10 11 12 13",
        ),
    ];
    for (option, text) in vectors {
        assert_eq!(option.to_vec(), hex(text), "encode {option:?}");
        let bytes = hex(text);
        let (decoded, used) = MptcpOption::parse(&bytes).expect("parse");
        assert_eq!(used, bytes.len());
        assert_eq!(decoded, option, "decode {text}");
    }
}

#[test]
fn dss_options_encode_byte_exact() {
    let ack = crate::dss::DataSequenceSignal {
        data_ack: Some(0x0102_0304),
        ack_64: false,
        mapping: None,
        data_fin: false,
    };
    assert_eq!(
        MptcpOption::Dss(ack).to_vec(),
        hex("1e 08 20 01 01 02 03 04")
    );

    let mapping = crate::dss::Mapping {
        data_seq: 0x1112_1314_1516_1718,
        subflow_seq: 0x2122_2324,
        data_len: 44,
        checksum: Some(0x3344),
        dsn_64: true,
    };
    let full = crate::dss::DataSequenceSignal {
        data_ack: Some(0x0102_0304_0506_0708),
        ack_64: true,
        mapping: Some(mapping),
        data_fin: true,
    };
    let text =
        "1e 1c 20 1f 01 02 03 04 05 06 07 08 11 12 13 14 15 16 17 18 21 22 23 24 00 2c 33 44";
    assert_eq!(MptcpOption::Dss(full.clone()).to_vec(), hex(text));
    let (decoded, _) = MptcpOption::parse(&hex(text)).expect("parse dss");
    assert_eq!(decoded, MptcpOption::Dss(full));
}

#[test]
fn address_and_control_options_encode_byte_exact() {
    let hmac = [0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7, 0xa8];
    let v4 = Address::V4([192, 0, 2, 1]);
    let v6 = Address::V6([0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    let vectors = [
        (
            add(false, 5, v4, None, Some(hmac)),
            "1e 10 30 05 c0 00 02 01 a1 a2 a3 a4 a5 a6 a7 a8",
        ),
        (add(true, 5, v4, None, None), "1e 08 31 05 c0 00 02 01"),
        (
            add(false, 5, v4, Some(8080), Some(hmac)),
            "1e 12 30 05 c0 00 02 01 1f 90 a1 a2 a3 a4 a5 a6 a7 a8",
        ),
        (
            add(false, 6, v6, None, Some(hmac)),
            "1e 1c 30 06 20 01 0d b8 00 00 00 00 00 00 00 00 00 00 00 01 a1 a2 a3 a4 a5 a6 a7 a8",
        ),
        (
            MptcpOption::RemoveAddr(RemoveIds::new(vec![3, 7, 9]).expect("ids")),
            "1e 06 40 03 07 09",
        ),
        (MptcpOption::MpPrio { backup: false }, "1e 03 50"),
        (MptcpOption::MpPrio { backup: true }, "1e 03 51"),
        (
            MptcpOption::MpFail {
                data_seq: 0x0102_0304_0506_0708,
            },
            "1e 0c 60 00 01 02 03 04 05 06 07 08",
        ),
        (
            MptcpOption::MpFastClose {
                receiver_key: 0x1112_1314_1516_1718,
            },
            "1e 0c 70 00 11 12 13 14 15 16 17 18",
        ),
        (
            MptcpOption::MpTcpRst {
                transient: false,
                reason: TcpRstReason::Mptcp,
            },
            "1e 04 80 01",
        ),
        (
            MptcpOption::MpTcpRst {
                transient: true,
                reason: TcpRstReason::LackOfResources,
            },
            "1e 04 81 02",
        ),
    ];
    for (option, text) in vectors {
        assert_eq!(option.to_vec(), hex(text), "encode {option:?}");
        let bytes = hex(text);
        let (decoded, used) = MptcpOption::parse(&bytes).expect("parse");
        assert_eq!(used, bytes.len());
        assert_eq!(decoded, option, "decode {text}");
    }
}

#[test]
fn every_prefix_of_every_vector_fails_closed() {
    let vectors = [
        "1e 04 01 01",
        "1e 0c 01 01 01 02 03 04 05 06 07 08",
        "1e 14 01 01 11 12 13 14 15 16 17 18 01 02 03 04 05 06 07 08",
        "1e 0c 10 03 11 22 33 44 55 66 77 88",
        "1e 10 10 02 00 01 02 03 04 05 06 07 99 aa bb cc",
        "1e 18 10 00 00 01 02 03 04 05 06 07 08 09 0a 0b 0c 0d 0e 0f 10 11 12 13",
        "1e 08 20 01 01 02 03 04",
        "1e 10 30 05 c0 00 02 01 a1 a2 a3 a4 a5 a6 a7 a8",
        "1e 06 40 03 07 09",
        "1e 03 50",
        "1e 0c 60 00 01 02 03 04 05 06 07 08",
        "1e 0c 70 00 11 12 13 14 15 16 17 18",
        "1e 04 80 01",
    ];
    for text in vectors {
        let bytes = hex(text);
        for prefix in 0..bytes.len() {
            let result = MptcpOption::parse(&bytes[..prefix]);
            assert!(
                result.is_err(),
                "prefix {prefix} of {text} was accepted: {result:?}"
            );
        }
    }
}

#[test]
fn malformed_and_unknown_options_are_rejected() {
    assert_eq!(MptcpOption::parse(&[]), Err(Error::Truncated));
    assert_eq!(MptcpOption::parse(&[29, 4, 0, 0, 0]), Err(Error::NotMptcp));
    assert_eq!(MptcpOption::parse(&[30, 2, 0]), Err(Error::BadLength));
    assert_eq!(
        MptcpOption::parse(&[30, 4, 0x90, 0]),
        Err(Error::UnknownSubtype)
    );
    assert_eq!(
        MptcpOption::parse(&hex("1e 04 02 01")),
        Err(Error::UnsupportedVersion)
    );
    assert_eq!(
        MptcpOption::parse(&hex("1e 04 01 00")),
        Err(Error::InvalidFlags)
    );
    assert_eq!(
        MptcpOption::parse(&hex("1e 0c 10 03 11 22 33 44 55 66 77 88")),
        Ok((
            MptcpOption::MpJoinSyn {
                backup: false,
                address_id: 3,
                token: 0x1122_3344,
                rand: 0x5566_7788,
            },
            12
        ))
    );
}

#[test]
fn option_list_skips_nop_and_non_mptcp() {
    let mut bytes = vec![1, 2, 4, 0x05, 0xb4];
    bytes.extend_from_slice(&hex("1e 04 01 01"));
    bytes.push(0);
    let options = parse_options(&bytes).expect("options");
    assert_eq!(options.len(), 1);
    assert_eq!(
        options[0],
        MptcpOption::MpCapable {
            version: VERSION,
            flags: MpCapableFlags::default(),
            sender_key: None,
            receiver_key: None,
            data_level_length: None,
            checksum: None,
        }
    );
}

#[test]
fn a_three_byte_dss_header_is_rejected_not_indexed() {
    // cargo-fuzz found [01 1e 03 20 ff ff 59]: a NOP then kind 30, length 3, DSS subtype. The
    // DSS flags byte does not exist, so the decoder must return BadLength instead of indexing
    // body[3].
    assert_eq!(MptcpOption::parse(&[30, 3, 0x20]), Err(Error::BadLength));
    assert_eq!(
        parse_options(&[0x01, 0x1e, 0x03, 0x20, 0xff, 0xff, 0x59]),
        Err(Error::BadLength)
    );
}

#[test]
fn a_three_byte_add_addr_header_is_rejected_not_indexed() {
    // cargo-fuzz found [01 1e 03 30]: NOP then ADD_ADDR with no Address ID byte.
    assert_eq!(MptcpOption::parse(&[30, 3, 0x30]), Err(Error::BadLength));
    assert_eq!(
        parse_options(&[0x01, 0x1e, 0x03, 0x30]),
        Err(Error::BadLength)
    );
}

#[test]
fn remove_ids_rejects_empty_and_oversized_lists() {
    assert_eq!(RemoveIds::new(vec![]), Err(Error::BadLength));
    assert_eq!(
        RemoveIds::new(vec![0; MAX_REMOVE_IDS + 1]),
        Err(Error::BadLength)
    );
    assert!(RemoveIds::new(vec![0; MAX_REMOVE_IDS]).is_ok());
}
