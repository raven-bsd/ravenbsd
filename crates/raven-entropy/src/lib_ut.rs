// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the entropy pool.

use super::*;

const RFC_KEY: [u8; 32] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];
const RFC_NONCE: [u8; 12] = [
    0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x4a, 0x00, 0x00, 0x00, 0x00,
];
const RFC_BLOCK: [u8; 64] = [
    0x10, 0xf1, 0xe7, 0xe4, 0xd1, 0x3b, 0x59, 0x15, 0x50, 0x0f, 0xdd, 0x1f, 0xa3, 0x20, 0x71, 0xc4,
    0xc7, 0xd1, 0xf4, 0xc7, 0x33, 0xc0, 0x68, 0x03, 0x04, 0x22, 0xaa, 0x9a, 0xc3, 0xd4, 0x6c, 0x4e,
    0xd2, 0x82, 0x64, 0x46, 0x07, 0x9f, 0xaa, 0x09, 0x14, 0xc2, 0xd7, 0x05, 0xd9, 0x8b, 0x02, 0xa2,
    0xb5, 0x12, 0x9c, 0xd1, 0xde, 0x16, 0x4e, 0xb9, 0xcb, 0xd0, 0x83, 0xe8, 0xa2, 0x50, 0x3c, 0x4e,
];

fn seed_bytes(value: u8) -> [u8; 64] {
    [value; 64]
}

#[test]
fn rfc_8439_known_answer() {
    let mut out = [0u8; 64];
    block(&RFC_KEY, 1, &RFC_NONCE, &mut out);
    assert_eq!(out, RFC_BLOCK);
}

#[test]
fn unseeded_fill_is_rejected() {
    let mut pool = Pool::new();
    assert!(!pool.is_seeded());
    let mut out = [0u8; 16];
    assert_eq!(pool.fill(&mut out), Err(Error::Unseeded));
    assert_eq!(out, [0u8; 16]);
}

#[test]
fn seed_then_fill_is_deterministic() {
    let mut first = Pool::new();
    first.add_entropy(&seed_bytes(0x5a));
    let mut a = [0u8; 64];
    first.fill(&mut a).unwrap();

    let mut second = Pool::new();
    second.add_entropy(&seed_bytes(0x5a));
    let mut b = [0u8; 64];
    second.fill(&mut b).unwrap();

    assert!(first.is_seeded());
    assert_eq!(a, b);
}

#[test]
fn different_seeds_differ() {
    let mut first = Pool::new();
    first.add_entropy(&seed_bytes(0x5a));
    let mut a = [0u8; 64];
    first.fill(&mut a).unwrap();

    let mut second = Pool::new();
    second.add_entropy(&seed_bytes(0xa5));
    let mut b = [0u8; 64];
    second.fill(&mut b).unwrap();

    assert_ne!(a, b);
}

#[test]
fn chunked_seed_matches_whole_seed() {
    let mut whole = Pool::new();
    whole.add_entropy(&seed_bytes(0x42));
    let mut a = [0u8; 64];
    whole.fill(&mut a).unwrap();

    let mut chunked = Pool::new();
    chunked.add_entropy(&seed_bytes(0x42)[..16]);
    chunked.add_entropy(&seed_bytes(0x42)[16..48]);
    chunked.add_entropy(&seed_bytes(0x42)[48..]);
    let mut b = [0u8; 64];
    chunked.fill(&mut b).unwrap();

    assert_eq!(a, b);
}

#[test]
fn rekey_changes_the_stream() {
    let mut pool = Pool::new();
    pool.add_entropy(&seed_bytes(0x11));
    let mut before = [0u8; 64];
    pool.fill(&mut before).unwrap();

    // A fresh accumulator forces a re-key and a different stream.
    pool.add_entropy(&seed_bytes(0x22));
    let mut after = [0u8; 64];
    pool.fill(&mut after).unwrap();

    assert_ne!(before, after);
}

#[test]
fn reseed_budget_is_enforced() {
    let mut pool = Pool::new();
    pool.add_entropy(&seed_bytes(0x33));
    // RESEED_INTERVAL is 64 KiB; fill 64 KiB minus one byte, then one more block is refused.
    let mut big = [0u8; RESEED_INTERVAL as usize - 1];
    assert_eq!(pool.fill(&mut big), Ok(RESEED_INTERVAL as usize - 1));
    let mut extra = [0u8; 64];
    assert_eq!(pool.fill(&mut extra), Err(Error::NeedsReseed));
}

#[test]
fn reseed_resets_the_budget() {
    let mut pool = Pool::new();
    pool.add_entropy(&seed_bytes(0x44));
    let mut big = [0u8; RESEED_INTERVAL as usize - 1];
    pool.fill(&mut big).unwrap();
    assert_eq!(pool.fill(&mut [0u8; 64]), Err(Error::NeedsReseed));
    // Re-key resets the budget.
    pool.add_entropy(&seed_bytes(0x55));
    let mut out = [0u8; 64];
    assert_eq!(pool.fill(&mut out), Ok(64));
}

#[test]
fn output_is_not_all_zero() {
    let mut pool = Pool::new();
    pool.add_entropy(&seed_bytes(0x77));
    let mut out = [0u8; 256];
    pool.fill(&mut out).unwrap();
    assert!(out.iter().any(|&byte| byte != 0));
}
