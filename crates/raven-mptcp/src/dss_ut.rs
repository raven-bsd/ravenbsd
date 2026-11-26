// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! DSS checksum, encoding and strict flag/length tests.

use super::*;

#[test]
fn mapping_checksum_matches_the_rfc_pseudo_header() {
    let payload: Vec<u8> = (0..44).collect();
    assert_eq!(
        checksum(0x1112_1314_1516_1718, 0x2122_2324, 44, &payload),
        0x9b53
    );
    assert!(verify_checksum(
        0x1112_1314_1516_1718,
        0x2122_2324,
        44,
        &payload,
        0x9b53
    ));
    let mut damaged = payload;
    damaged[0] ^= 1;
    assert!(!verify_checksum(
        0x1112_1314_1516_1718,
        0x2122_2324,
        44,
        &damaged,
        0x9b53
    ));
}

#[test]
fn ack_and_mapping_round_trip_in_32_bit_form() {
    let signal = DataSequenceSignal {
        data_ack: Some(0x1122_3344),
        ack_64: false,
        mapping: Some(Mapping {
            data_seq: 0x5566_7788,
            subflow_seq: 0x99aa_bbcc,
            data_len: 12,
            checksum: None,
            dsn_64: false,
        }),
        data_fin: false,
    };
    let body = signal.encode_body();
    assert_eq!(body.len(), 14);
    let flags = signal.flags();
    assert_eq!(flags, FLAG_DATA_ACK | FLAG_HAS_MAP);
    let parsed = DataSequenceSignal::parse_body(&body, flags, 4 + body.len()).expect("parse");
    assert_eq!(parsed, signal);
}

#[test]
fn infinite_mapping_requires_a_zero_checksum() {
    let mut signal = DataSequenceSignal {
        data_ack: None,
        ack_64: false,
        mapping: Some(Mapping {
            data_seq: 7,
            subflow_seq: 0,
            data_len: 0,
            checksum: Some(0),
            dsn_64: true,
        }),
        data_fin: false,
    };
    let body = signal.encode_body();
    let flags = signal.flags();
    assert!(DataSequenceSignal::parse_body(&body, flags, 4 + body.len()).is_ok());
    if let Some(mapping) = &mut signal.mapping {
        mapping.checksum = Some(1);
    }
    let body = signal.encode_body();
    assert_eq!(
        DataSequenceSignal::parse_body(&body, flags, 4 + body.len()),
        Err(crate::Error::InvalidDssFlags)
    );
}

#[test]
fn invalid_flag_combinations_are_rejected() {
    // `a` without `A`.
    assert_eq!(
        DataSequenceSignal::parse_body(&[], FLAG_ACK_64, 4),
        Err(crate::Error::InvalidDssFlags)
    );
    // `m` without `M`.
    assert_eq!(
        DataSequenceSignal::parse_body(&[], FLAG_DSN_64, 4),
        Err(crate::Error::InvalidDssFlags)
    );
    // DATA_FIN without a mapping.
    assert_eq!(
        DataSequenceSignal::parse_body(&[], FLAG_DATA_FIN, 4),
        Err(crate::Error::InvalidDssFlags)
    );
    // Reserved high flag bits.
    assert_eq!(
        DataSequenceSignal::parse_body(&[], 0x20, 4),
        Err(crate::Error::InvalidDssFlags)
    );
}

#[test]
fn a_length_without_the_declared_fields_is_rejected() {
    // ACK64 + map64 requires 26 bytes (28 with checksum); 27 is not a valid length.
    assert_eq!(
        DataSequenceSignal::parse_body(
            &[0u8; 23],
            FLAG_DATA_ACK | FLAG_ACK_64 | FLAG_HAS_MAP | FLAG_DSN_64,
            27
        ),
        Err(crate::Error::BadLength)
    );
}
