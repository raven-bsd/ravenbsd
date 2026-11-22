// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! SHA-256/HMAC vectors and MPTCP key derivations.

use super::*;

fn hex(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut index = 0;
    while index + 1 < bytes.len() {
        let high = (bytes[index] as char).to_digit(16).expect("high nibble");
        let low = (bytes[index + 1] as char).to_digit(16).expect("low nibble");
        out.push(((high << 4) | low) as u8);
        index += 2;
    }
    out
}

fn digest(text: &str) -> [u8; 32] {
    let bytes = hex(text);
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    out
}

#[test]
fn sha256_matches_fips_vectors() {
    assert_eq!(
        sha256(b""),
        digest("e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855")
    );
    assert_eq!(
        sha256(b"abc"),
        digest("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert_eq!(
        sha256(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
        digest("248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1")
    );
}

#[test]
fn hmac_matches_rfc4231_case_two() {
    assert_eq!(
        hmac_sha256(b"Jefe", b"what do ya want for nothing?"),
        digest("5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843")
    );
}

#[test]
fn hmac_handles_a_long_key() {
    let key = [0xaau8; 131];
    let expected = {
        let hashed = sha256(&key);
        hmac_sha256(
            &hashed,
            b"Test Using Larger Than Block-Size Key - Hash Key First",
        )
    };
    assert_eq!(
        hmac_sha256(
            &key,
            b"Test Using Larger Than Block-Size Key - Hash Key First"
        ),
        expected
    );
}

#[test]
fn token_and_idsn_use_network_order() {
    // SHA-256 of key 0x0102030405060708 (big-endian).
    let key = 0x0102_0304_0506_0708;
    assert_eq!(token(key), 0x6684_0dda);
    assert_eq!(initial_data_seq(key), 0xf5a1_01d3_d29d_6f72);
}

#[test]
fn tag_comparison_is_exact() {
    assert!(tag_eq(&[1, 2, 3], &[1, 2, 3]));
    assert!(!tag_eq(&[1, 2, 3], &[1, 2, 4]));
    assert!(!tag_eq(&[1, 2], &[1, 2, 3]));
}
