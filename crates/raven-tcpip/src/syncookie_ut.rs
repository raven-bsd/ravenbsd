// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the FreeBSD SYN cookie port (K304).

use super::*;

const KEY: [u8; SYNCOOKIE_SECRET_SIZE] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
];

#[test]
fn siphash24_matches_the_published_vectors() {
    let expected = [
        0x726f_db47_dd0e_0e31,
        0x74f8_39c5_93dc_67fd,
        0x0d6c_8009_d9a9_4f5a,
        0x8567_6696_d7fb_7e2d,
        0xcf27_94e0_2771_87b7,
        0x1876_5564_cd99_a68d,
        0xcbc9_466e_58fe_e3ce,
        0xab02_00f5_8b01_d137,
        0x93f5_f579_9a93_2462,
        0x9e00_82df_0ba9_e4b0,
        0x7a5d_bbc5_94dd_b9f3,
        0xf4b3_2f46_226b_ada7,
        0x751e_8fbc_860e_e5fb,
        0x14ea_5627_c084_3d90,
        0xf723_ca90_8e7a_f2ee,
        0xa129_ca61_49be_45e5,
    ];
    let mut message = [0u8; 16];
    for (index, byte) in message.iter_mut().enumerate() {
        *byte = index as u8;
    }
    for (length, digest) in expected.iter().enumerate() {
        assert_eq!(
            siphash24(&KEY, &message[..length]),
            *digest,
            "length {length}"
        );
    }
}

#[test]
fn the_donor_tables_are_exact() {
    assert_eq!(
        MSS_TABLE,
        [216, 536, 1_200, 1_360, 1_400, 1_440, 1_452, 1_460]
    );
    assert_eq!(WSCALE_TABLE, [0, 0, 1, 2, 4, 6, 7, 8]);
    assert_eq!(SYNCOOKIE_SECRET_SIZE, 16);
    assert_eq!(SYNCOOKIE_LIFETIME_MS, 15_000);
    assert_eq!(TCP_MAX_WINDOW, 65_535);
}

#[test]
fn the_table_index_rounds_down() {
    assert_eq!(mss_index(100), 0);
    assert_eq!(mss_index(216), 0);
    assert_eq!(mss_index(217), 0);
    assert_eq!(mss_index(536), 1);
    assert_eq!(mss_index(1_401), 4);
    assert_eq!(mss_index(9_000), 7);

    assert_eq!(wscale_index(0), 1);
    assert_eq!(wscale_index(1), 2);
    assert_eq!(wscale_index(3), 3);
    assert_eq!(wscale_index(14), 7);
}

#[test]
fn reseeding_replaces_the_inactive_key_and_flips_the_parity() {
    let mut secret = SyncookieSecret::new([0x11; 16], [0x22; 16]);
    assert_eq!(secret.parity(), 0);
    assert_eq!(secret.key(), &[0x11; 16]);

    secret.reseed([0x33; 16]);
    assert_eq!(secret.parity(), 1);
    assert_eq!(secret.key(), &[0x33; 16]);

    secret.reseed([0x44; 16]);
    assert_eq!(secret.parity(), 0);
    assert_eq!(secret.key(), &[0x44; 16]);
    assert_eq!(secret.key_for_parity(1), &[0x33; 16]);
}

#[test]
fn the_mac_covers_the_whole_four_tuple_and_secmod() {
    let conn = CookieConn::new(
        IpAddr::V4(crate::Ipv4Addr::new([192, 0, 2, 1])),
        40_000,
        IpAddr::V4(crate::Ipv4Addr::new([198, 51, 100, 2])),
        80,
    );
    let base = mac(&conn, 0x1122_3344, 0xfe, &KEY, 0x1234);
    let mut other = conn;
    other.local_port += 1;
    assert_ne!(base, mac(&other, 0x1122_3344, 0xfe, &KEY, 0x1234));
    assert_ne!(base, mac(&conn, 0x1122_3345, 0xfe, &KEY, 0x1234));
    assert_ne!(base, mac(&conn, 0x1122_3344, 0xff, &KEY, 0x1234));
    assert_ne!(base, mac(&conn, 0x1122_3344, 0xfe, &KEY, 0x1235));
}
