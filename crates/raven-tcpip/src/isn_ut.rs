// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the FreeBSD ISN/SipHash port (K278).

use super::*;
use crate::{IpAddr, Ipv4Addr, Ipv6Addr};

const SECRET: [u8; 16] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
];

fn conn4() -> IsnConn {
    IsnConn::new(
        IpAddr::V4(Ipv4Addr::new([192, 0, 2, 1])),
        40_000,
        IpAddr::V4(Ipv4Addr::new([198, 51, 100, 2])),
        80,
    )
}

fn conn6() -> IsnConn {
    IsnConn::new(
        IpAddr::V6(Ipv6Addr::new([
            0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1,
        ])),
        40_000,
        IpAddr::V6(Ipv6Addr::new([
            0x20, 0x01, 0x0d, 0xb8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
        ])),
        443,
    )
}

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
            siphash24(&SECRET, &message[..length]),
            *digest,
            "length {length}"
        );
    }
}

#[test]
fn keyed_hash_xors_the_two_sixty_four_bit_halves() {
    let conn = conn4();
    let (bytes, length) = conn.hash_input();
    let digest = siphash24(&SECRET, &bytes[..length]);
    assert_eq!(
        keyed_hash(&conn, &SECRET),
        (digest as u32) ^ ((digest >> 32) as u32)
    );
}

#[test]
fn keyed_hash_rejects_a_short_key_and_ignores_key_bytes_past_sixteen() {
    let conn = conn4();
    assert!(keyed_hash_slice(&conn, &[0u8; 15]).is_none());
    assert!(keyed_hash_slice(&conn, &[0u8; 16]).is_some());
    let mut key = [0u8; 17];
    key[16] = 0x55;
    assert_eq!(
        keyed_hash_slice(&conn, &key),
        keyed_hash_slice(&conn, &[0u8; 16])
    );
}

#[test]
fn the_hash_input_is_remote_port_local_port_remote_address_local_address() {
    let (bytes, length) = conn4().hash_input();
    assert_eq!(
        &bytes[..length],
        &[0x00, 0x50, 0x9c, 0x40, 198, 51, 100, 2, 192, 0, 2, 1]
    );
    let (bytes, length) = conn6().hash_input();
    assert_eq!(length, 2 + 2 + 16 + 16);
    assert_eq!(&bytes[..4], &[0x01, 0xbb, 0x9c, 0x40]);
}

#[test]
fn the_first_use_reseeds_and_uses_the_caller_secret() {
    let state = IsnState::new(100, [0x11; 16], [0x22; 16]);
    assert!(state.reseed_due(0));
    assert!(state.reseed_due(1));

    let mut state = IsnState::new(100, [0x11; 16], [0x22; 16]);
    let _ = state.next_isn(&conn4(), 1, IsnEntropy::new(SECRET, 0));
    assert_eq!(state.last_reseed(), 1);
    assert!(!state.reseed_due(1));
}

#[test]
fn the_reseed_interval_is_strictly_greater_than_the_interval() {
    let mut state = IsnState::new(100, SECRET, SECRET);
    state.last_reseed = 5;
    state.set_reseed_interval_secs(10);
    assert!(!state.reseed_due(1_005));
    assert!(state.reseed_due(1_006));
}

#[test]
fn the_offset_increment_wraps_around_u32() {
    let mut state = IsnState::new(1024, SECRET, SECRET);
    state.last_reseed = 1;
    state.last_tick = 5;
    state.offset = u32::MAX - 100;
    state.offset_old = state.offset;
    let isn = state.next_isn(&conn4(), 5, IsnEntropy::new(SECRET, 0));
    assert_eq!(state.offset(), 3_995);
    assert_eq!(isn, keyed_hash(&conn4(), &SECRET).wrapping_add(3_995));
}

#[test]
fn the_tick_projection_wraps_around_u32() {
    let mut state = IsnState::new(1, SECRET, SECRET);
    state.last_reseed = 1;
    state.last_tick = 0;
    state.offset = u32::MAX - 1_000;
    state.offset_old = state.offset;
    let _ = state.next_isn(&conn4(), 1, IsnEntropy::new(SECRET, 0));
    assert_eq!(state.offset(), 1_047_575);
}

#[test]
fn the_projection_is_skipped_within_one_tick() {
    let mut state = IsnState::new(1024, SECRET, SECRET);
    state.last_reseed = 1;
    state.last_tick = 10;
    state.offset = 100_000;
    state.offset_old = 100_000;
    let _ = state.next_isn(&conn4(), 10, IsnEntropy::new(SECRET, 0));
    assert_eq!(state.offset(), 104_096);
}
