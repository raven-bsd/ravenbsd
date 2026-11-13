// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

const KEY: [u8; 32] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];
const NONCE: [u8; 12] = [
    0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x4a, 0x00, 0x00, 0x00, 0x00,
];
const RFC_BLOCK: [u8; 64] = [
    0x10, 0xf1, 0xe7, 0xe4, 0xd1, 0x3b, 0x59, 0x15, 0x50, 0x0f, 0xdd, 0x1f, 0xa3, 0x20, 0x71, 0xc4,
    0xc7, 0xd1, 0xf4, 0xc7, 0x33, 0xc0, 0x68, 0x03, 0x04, 0x22, 0xaa, 0x9a, 0xc3, 0xd4, 0x6c, 0x4e,
    0xd2, 0x82, 0x64, 0x46, 0x07, 0x9f, 0xaa, 0x09, 0x14, 0xc2, 0xd7, 0x05, 0xd9, 0x8b, 0x02, 0xa2,
    0xb5, 0x12, 0x9c, 0xd1, 0xde, 0x16, 0x4e, 0xb9, 0xcb, 0xd0, 0x83, 0xe8, 0xa2, 0x50, 0x3c, 0x4e,
];

fn djb_counter_one() -> ChaCha {
    let mut chacha = ChaCha::zeroed();
    chacha.keysetup(&KEY);
    chacha.ivsetup(&[0; 8]);
    chacha.input[12] = 1;
    chacha.input[13] = u32::from_le_bytes([NONCE[0], NONCE[1], NONCE[2], NONCE[3]]);
    chacha.input[14] = u32::from_le_bytes([NONCE[4], NONCE[5], NONCE[6], NONCE[7]]);
    chacha.input[15] = u32::from_le_bytes([NONCE[8], NONCE[9], NONCE[10], NONCE[11]]);
    chacha
}

#[test]
fn rfc_8439_block_known_answer() {
    let mut chacha = djb_counter_one();
    let mut out = [0u8; 64];
    chacha.keystream_bytes(&mut out);
    assert_eq!(out, RFC_BLOCK);
}

#[test]
fn zeroed_state_is_all_zero() {
    assert_eq!(ChaCha::zeroed().words(), [0; 16]);
}

#[test]
fn keysetup_writes_sigma_and_key() {
    let mut chacha = ChaCha::zeroed();
    chacha.keysetup(&KEY);
    let words = chacha.words();
    assert_eq!(
        &words[0..4],
        &[0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574]
    );
    assert_eq!(words[4], u32::from_le_bytes([0, 1, 2, 3]));
}

#[test]
fn ivsetup_resets_the_counter() {
    let mut chacha = ChaCha::zeroed();
    chacha.keysetup(&KEY);
    chacha.input[12] = 7;
    chacha.input[13] = 8;
    chacha.ivsetup(&[1, 2, 3, 4, 5, 6, 7, 8]);
    let words = chacha.words();
    assert_eq!(words[12], 0);
    assert_eq!(words[13], 0);
    assert_eq!(words[14], u32::from_le_bytes([1, 2, 3, 4]));
    assert_eq!(words[15], u32::from_le_bytes([5, 6, 7, 8]));
}

#[test]
fn empty_request_does_not_advance_the_counter() {
    let mut chacha = djb_counter_one();
    let before = chacha.words()[12];
    chacha.keystream_bytes(&mut []);
    assert_eq!(chacha.words()[12], before);
}

#[test]
fn one_block_advances_the_counter_once() {
    let mut chacha = djb_counter_one();
    let mut out = [0u8; 64];
    chacha.keystream_bytes(&mut out);
    assert_eq!(chacha.words()[12], 2);
    assert_eq!(chacha.words()[13], 0x0900_0000);
}

#[test]
fn split_requests_equal_one_long_stream() {
    let mut long = ChaCha::zeroed();
    long.keysetup(&KEY);
    long.ivsetup(&[9; 8]);
    let mut expected = [0u8; 2048];
    long.keystream_bytes(&mut expected);

    let mut split = ChaCha::zeroed();
    split.keysetup(&KEY);
    split.ivsetup(&[9; 8]);
    let mut actual = [0u8; 2048];
    let mut offset = 0usize;
    for length in [1usize, 7, 63, 64, 65, 1024, 824] {
        let end = offset + length;
        split.keystream_bytes(&mut actual[offset..end]);
        offset = end;
    }
    assert_eq!(actual, expected);
}

#[test]
fn first_block_matches_raven_prng_zero_key_zero_nonce() {
    let mut chacha = ChaCha::zeroed();
    chacha.keysetup(&[0; 32]);
    chacha.ivsetup(&[0; 8]);
    let mut actual = [0u8; 64];
    chacha.keystream_bytes(&mut actual);
    let mut expected = [0u8; 64];
    raven_prng::block(&[0; 32], 0, &[0; 12], &mut expected);
    assert_eq!(actual, expected);
}

#[test]
fn one_thousand_keystreams_match_raven_prng_blocks() {
    for seed in 0u64..1000 {
        let mut key = [0u8; 32];
        let mut iv = [0u8; 8];
        let mut value = seed | 1;
        for byte in &mut key {
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            *byte = value as u8;
        }
        for byte in &mut iv {
            value ^= value << 13;
            value ^= value >> 7;
            value ^= value << 17;
            *byte = value as u8;
        }
        let mut chacha = ChaCha::zeroed();
        chacha.keysetup(&key);
        chacha.ivsetup(&iv);
        let mut actual = [0u8; 1024];
        chacha.keystream_bytes(&mut actual);
        let mut expected = [0u8; 1024];
        let mut nonce = [0u8; 12];
        nonce[4..].copy_from_slice(&iv);
        for block in 0..16u32 {
            let mut out = [0u8; 64];
            raven_prng::block(&key, block, &nonce, &mut out);
            expected[block as usize * 64..(block as usize + 1) * 64].copy_from_slice(&out);
        }
        assert_eq!(actual, expected, "seed {seed}");
    }
}

#[test]
fn counter_carry_moves_into_word_thirteen() {
    let mut chacha = ChaCha::zeroed();
    chacha.keysetup(&KEY);
    chacha.ivsetup(&[0; 8]);
    chacha.input[12] = u32::MAX;
    let mut out = [0u8; 128];
    chacha.keystream_bytes(&mut out);
    assert_eq!(chacha.words()[12], 1);
    assert_eq!(chacha.words()[13], 1);
}

#[test]
fn byte_at_a_time_matches_one_call() {
    let mut one = ChaCha::zeroed();
    one.keysetup(&KEY);
    one.ivsetup(&[4; 8]);
    let mut expected = [0u8; 128];
    one.keystream_bytes(&mut expected);

    let mut byte = ChaCha::zeroed();
    byte.keysetup(&KEY);
    byte.ivsetup(&[4; 8]);
    let mut actual = [0u8; 128];
    for slot in &mut actual {
        let mut single = [0u8; 1];
        byte.keystream_bytes(&mut single);
        *slot = single[0];
    }
    assert_eq!(actual, expected);
}

#[test]
fn keysetup_restarts_the_stream() {
    let mut chacha = ChaCha::zeroed();
    chacha.keysetup(&KEY);
    chacha.ivsetup(&[5; 8]);
    let mut first = [0u8; 64];
    chacha.keystream_bytes(&mut first);
    chacha.keysetup(&KEY);
    chacha.ivsetup(&[5; 8]);
    let mut again = [0u8; 64];
    chacha.keystream_bytes(&mut again);
    assert_eq!(first, again);
}

#[test]
fn ivsetup_restarts_the_stream() {
    let mut chacha = ChaCha::zeroed();
    chacha.keysetup(&KEY);
    chacha.ivsetup(&[6; 8]);
    let mut first = [0u8; 64];
    chacha.keystream_bytes(&mut first);
    chacha.ivsetup(&[6; 8]);
    let mut again = [0u8; 64];
    chacha.keystream_bytes(&mut again);
    assert_eq!(first, again);
}

#[test]
fn two_blocks_match_two_raven_prng_blocks() {
    let mut chacha = ChaCha::zeroed();
    chacha.keysetup(&KEY);
    chacha.ivsetup(&[7; 8]);
    let mut actual = [0u8; 128];
    chacha.keystream_bytes(&mut actual);
    let mut expected = [0u8; 128];
    let mut nonce = [0u8; 12];
    nonce[4..].copy_from_slice(&[7; 8]);
    let mut first = [0u8; 64];
    let mut second = [0u8; 64];
    raven_prng::block(&KEY, 0, &nonce, &mut first);
    raven_prng::block(&KEY, 1, &nonce, &mut second);
    expected[..64].copy_from_slice(&first);
    expected[64..].copy_from_slice(&second);
    assert_eq!(actual, expected);
}

#[test]
fn ivsetup_writes_the_last_two_words() {
    let mut chacha = ChaCha::zeroed();
    chacha.keysetup(&KEY);
    chacha.ivsetup(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88]);
    let words = chacha.words();
    assert_eq!(words[14], 0x4433_2211);
    assert_eq!(words[15], 0x8877_6655);
}
