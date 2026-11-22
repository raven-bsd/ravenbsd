// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! K523 unit tests: OpenBSD `arc4random` construction.
//!
//! Requirement rows:
//!
//! * R01 - RFC 8439 known-answer and the original 8-byte-IV ChaCha20 keystream;
//! * R02 - unseeded pool and a failed/rejected seed are defined errors;
//! * R03 - randomized re-key interval and a clean reseed boundary (no repeated output);
//! * R04 - the fork child is zeroed, unseeded and diverges from the parent;
//! * R05 - key, IV, keystream buffer and counters are wiped;
//! * R06 - timer jitter is never accepted as a seed or a re-key.
//!
//! The construction vectors below were produced by an independent RFC 8439
//! ChaCha20 implementation (`cryptography` 46.0.5) configured with the donor's
//! state layout `state[12..16] = counter64_le || iv64_le`, then the OpenBSD
//! `_rs_init`/`_rs_stir`/`_rs_rekey` buffer logic was applied over it. The
//! zero-key/zero-IV block is the published ChaCha20 block-0 vector; the
//! independent output was cross-checked against it. Donor revision:
//! OpenBSD `4ce71f2817f675745b0596aaf04de4a122b1cc52`
//! (`arc4random.c,v 1.58`, `arc4random.h,v 1.4`, `chacha_private.h,v 1.3`).

use super::*;

const RFC_KEY: [u8; KEY_SIZE] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
];
const RFC_NONCE: [u8; NONCE_SIZE] = [
    0x00, 0x00, 0x00, 0x09, 0x00, 0x00, 0x00, 0x4a, 0x00, 0x00, 0x00, 0x00,
];
const RFC_BLOCK: [u8; BLOCK_SIZE] = [
    0x10, 0xf1, 0xe7, 0xe4, 0xd1, 0x3b, 0x59, 0x15, 0x50, 0x0f, 0xdd, 0x1f, 0xa3, 0x20, 0x71, 0xc4,
    0xc7, 0xd1, 0xf4, 0xc7, 0x33, 0xc0, 0x68, 0x03, 0x04, 0x22, 0xaa, 0x9a, 0xc3, 0xd4, 0x6c, 0x4e,
    0xd2, 0x82, 0x64, 0x46, 0x07, 0x9f, 0xaa, 0x09, 0x14, 0xc2, 0xd7, 0x05, 0xd9, 0x8b, 0x02, 0xa2,
    0xb5, 0x12, 0x9c, 0xd1, 0xde, 0x16, 0x4e, 0xb9, 0xcb, 0xd0, 0x83, 0xe8, 0xa2, 0x50, 0x3c, 0x4e,
];

/// Original ChaCha20, zero key, zero 8-byte IV, block counter 0.
const CHACHA_IV8_ZERO_BLOCK0: [u8; BLOCK_SIZE] = [
    0x76, 0xb8, 0xe0, 0xad, 0xa0, 0xf1, 0x3d, 0x90, 0x40, 0x5d, 0x6a, 0xe5, 0x53, 0x86, 0xbd, 0x28,
    0xbd, 0xd2, 0x19, 0xb8, 0xa0, 0x8d, 0xed, 0x1a, 0xa8, 0x36, 0xef, 0xcc, 0x8b, 0x77, 0x0d, 0xc7,
    0xda, 0x41, 0x59, 0x7c, 0x51, 0x57, 0x48, 0x8d, 0x77, 0x24, 0xe0, 0x3f, 0xb8, 0xd8, 0x4a, 0x37,
    0x6a, 0x43, 0xb8, 0xf4, 0x15, 0x18, 0xa1, 0x1c, 0xc3, 0x87, 0xb6, 0x69, 0xb2, 0xee, 0x65, 0x86,
];

/// Original ChaCha20, zero key, zero 8-byte IV, block counter 1.
const CHACHA_IV8_ZERO_BLOCK1: [u8; BLOCK_SIZE] = [
    0x9f, 0x07, 0xe7, 0xbe, 0x55, 0x51, 0x38, 0x7a, 0x98, 0xba, 0x97, 0x7c, 0x73, 0x2d, 0x08, 0x0d,
    0xcb, 0x0f, 0x29, 0xa0, 0x48, 0xe3, 0x65, 0x69, 0x12, 0xc6, 0x53, 0x3e, 0x32, 0xee, 0x7a, 0xed,
    0x29, 0xb7, 0x21, 0x76, 0x9c, 0xe6, 0x4e, 0x43, 0xd5, 0x71, 0x33, 0xb0, 0x74, 0xd8, 0x39, 0xd5,
    0x31, 0xed, 0x1f, 0x28, 0x51, 0x0a, 0xfb, 0x45, 0xac, 0xe1, 0x0a, 0x1f, 0x4b, 0x79, 0x4d, 0x6f,
];

// OpenBSD construction vector for seed 00 01 02 ... 27 (key 00..1f, IV 20..27).
// `stir` consumes the first four ChaCha bytes (bff36206) as the re-key fuzz;
// rs_count is therefore 1,242,047. The first output begins at rs_buf[40] after
// the first `_rs_rekey`, and the post-984-byte slice is after the second.
const OPENBSD_SEED: [u8; SEED_SIZE] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27,
];
const OPENBSD_RS_COUNT: u64 = 1_242_047;
const OPENBSD_FIRST_64: [u8; 64] = [
    0x72, 0xdc, 0x08, 0xd6, 0x40, 0xd4, 0x80, 0x27, 0xae, 0xbb, 0xe0, 0xec, 0x96, 0x24, 0x3a, 0xb3,
    0x91, 0xad, 0x81, 0x9c, 0xf4, 0x77, 0x0c, 0x82, 0x30, 0x1f, 0x90, 0xfc, 0xcc, 0xc7, 0x77, 0xad,
    0xc6, 0x07, 0xc8, 0xea, 0x93, 0xce, 0x45, 0x51, 0xcb, 0xdb, 0x51, 0x05, 0x87, 0x73, 0x3f, 0x14,
    0xa9, 0x01, 0x27, 0x6d, 0x49, 0xf5, 0xff, 0x36, 0x9f, 0x1f, 0x9c, 0x6d, 0x6a, 0x7a, 0x72, 0x1b,
];
const OPENBSD_SECOND_64: [u8; 64] = [
    0x61, 0xe8, 0x1b, 0x47, 0x8e, 0x15, 0xf8, 0xca, 0x72, 0x53, 0xe1, 0xef, 0xf0, 0xa8, 0x98, 0x58,
    0xdd, 0x1d, 0x5b, 0x35, 0x1c, 0x95, 0x36, 0x00, 0x4f, 0x0c, 0x4f, 0x08, 0x82, 0xae, 0x31, 0x34,
    0x80, 0x2b, 0xea, 0xd2, 0x2a, 0x8d, 0x2d, 0x2f, 0xdb, 0x8c, 0x9f, 0x34, 0x3a, 0xf2, 0x40, 0x59,
    0x3f, 0xcb, 0x47, 0xfd, 0xf2, 0x94, 0x0c, 0x24, 0xec, 0xf1, 0x60, 0x89, 0x33, 0xd1, 0xef, 0x03,
];
const OPENBSD_AFTER_984: [u8; 64] = [
    0x8a, 0xb2, 0x68, 0x7d, 0xe1, 0x9f, 0x8e, 0x3b, 0xf0, 0xcf, 0x78, 0x41, 0x97, 0x38, 0x8e, 0x76,
    0x30, 0x54, 0x12, 0x16, 0xe4, 0x81, 0x27, 0xdf, 0x87, 0x9c, 0xd5, 0x1a, 0xeb, 0xd4, 0xe6, 0xb4,
    0x75, 0x02, 0x1a, 0x0f, 0x57, 0x2f, 0x51, 0x49, 0x02, 0x99, 0x85, 0xbf, 0x7b, 0x5c, 0x1f, 0x9a,
    0x5f, 0xf0, 0x59, 0xf8, 0x8d, 0xad, 0x1b, 0x2b, 0x50, 0x02, 0xe2, 0x3f, 0xf1, 0x86, 0x09, 0xfe,
];

/// Sixteen different seeds' OpenBSD re-key intervals, all in `[1 MiB, 2 MiB)`.
const INTERVALS: [u64; 16] = [
    1_095_798, 1_196_692, 2_079_218, 1_594_002, 1_223_649, 1_505_822, 1_678_445, 1_094_514,
    1_953_476, 1_986_014, 1_865_132, 1_384_194, 1_144_592, 2_054_658, 2_084_299, 1_426_397,
];

fn seed_bytes(value: u8) -> [u8; SEED_SIZE] {
    [value; SEED_SIZE]
}

fn seeded(seed: &[u8; SEED_SIZE]) -> Pool {
    let mut pool = Pool::new();
    assert_eq!(pool.stir(SeedSource::GetEntropy, seed), Ok(()));
    pool
}

// K523-R01: the RFC 8439 ChaCha20 block function, 12-byte nonce and 32-bit counter.
#[test]
fn r01_rfc_8439_known_answer() {
    let mut out = [0u8; BLOCK_SIZE];
    block(&RFC_KEY, 1, &RFC_NONCE, &mut out);
    assert_eq!(out, RFC_BLOCK);
}

// K523-R01: the donor's original ChaCha variant, 8-byte IV and 64-bit counter.
#[test]
fn r01_chacha_iv8_zero_key_vector() {
    let mut out = [0u8; BLOCK_SIZE];
    block_iv8(&[0u8; KEY_SIZE], 0, &[0u8; IV_SIZE], &mut out);
    assert_eq!(out, CHACHA_IV8_ZERO_BLOCK0);
    block_iv8(&[0u8; KEY_SIZE], 1, &[0u8; IV_SIZE], &mut out);
    assert_eq!(out, CHACHA_IV8_ZERO_BLOCK1);
}

// K523-R01: the whole OpenBSD stir/rekey/buffer layout against an independent oracle.
#[test]
fn r01_openbsd_construction_reference_vector() {
    let mut pool = seeded(&OPENBSD_SEED);
    assert_eq!(pool.rs_count, OPENBSD_RS_COUNT);

    let mut first = [0u8; 64];
    assert_eq!(pool.fill(&mut first), Ok(64));
    assert_eq!(first, OPENBSD_FIRST_64);

    let mut second = [0u8; 64];
    assert_eq!(pool.fill(&mut second), Ok(64));
    assert_eq!(second, OPENBSD_SECOND_64);

    let mut rest = [0u8; RS_BUFSZ - SEED_SIZE - 128];
    assert_eq!(pool.fill(&mut rest), Ok(rest.len()));
    // OpenBSD rekeys eagerly when the copy exhausts the buffer, even at the exact end of the
    // request: the donor loop tests `rs_have == 0` after the copy, before the `n > 0` condition.
    assert_eq!(pool.rs_have, RS_BUFSZ - SEED_SIZE);

    let mut after = [0u8; 64];
    assert_eq!(pool.fill(&mut after), Ok(64));
    assert_eq!(after, OPENBSD_AFTER_984);
    assert_ne!(after, first);
}

// K523-R02: an unseeded pool returns a defined error and leaves output untouched.
#[test]
fn r02_unseeded_pool_is_a_defined_error() {
    let mut pool = Pool::new();
    assert!(!pool.is_seeded());
    let mut out = [0xa5u8; 32];
    assert_eq!(pool.fill(&mut out), Err(Error::Unseeded));
    assert_eq!(out, [0xa5u8; 32]);
}

// K523-R02: a rejected seed does not seed the pool.
#[test]
fn r02_seed_failure_leaves_pool_unseeded() {
    let mut pool = Pool::new();
    assert_eq!(
        pool.stir(SeedSource::TimerJitter, &seed_bytes(0x11)),
        Err(Error::WeakSource)
    );
    assert!(!pool.is_seeded());
    assert_eq!(pool.fill(&mut [0u8; 8]), Err(Error::Unseeded));

    assert_eq!(pool.stir(SeedSource::Hardware, &seed_bytes(0x22)), Ok(()));
    assert!(pool.is_seeded());
    assert_eq!(pool.fill(&mut [0u8; 8]), Ok(8));
}

// K523-R03: the interval is drawn from the fresh stream and stays near REKEY_BASE.
#[test]
fn r03_rekey_interval_is_randomized_near_base() {
    let mut intervals = [0u64; INTERVALS.len()];
    for (index, expected) in INTERVALS.iter().enumerate() {
        let pool = seeded(&seed_bytes(index as u8));
        assert_eq!(pool.rs_count, *expected);
        assert!(pool.rs_count >= REKEY_BASE);
        assert!(pool.rs_count < REKEY_BASE + REKEY_BASE);
        intervals[index] = pool.rs_count;
    }
    let mut distinct = 0usize;
    for index in 0..intervals.len() {
        if !intervals[..index].contains(&intervals[index]) {
            distinct += 1;
        }
    }
    assert_eq!(distinct, intervals.len(), "re-key intervals repeated");
}

// K523-R03: filling the interval to its last byte succeeds; one more byte requires a fresh seed.
#[test]
fn r03_boundary_requires_reseed() {
    let mut pool = seeded(&seed_bytes(0x33));
    let count = pool.rs_count;
    let mut body = std::vec![0u8; count as usize - 1];
    assert_eq!(pool.fill(&mut body), Ok(body.len()));
    assert_eq!(pool.rs_count, 1);

    let mut one = [0u8; 1];
    assert_eq!(pool.fill(&mut one), Err(Error::NeedsReseed));
    assert_eq!(one, [0]);

    assert_eq!(pool.stir(SeedSource::GetEntropy, &seed_bytes(0x44)), Ok(()));
    assert!(pool.rs_count >= REKEY_BASE);
    assert_eq!(pool.fill(&mut one), Ok(1));
}

// K523-R03: neither the 1,024-byte buffer refill nor a same-seed reseed repeats output.
#[test]
fn r03_no_repeated_output_across_boundaries() {
    let mut pool = seeded(&seed_bytes(0x42));
    let count = pool.rs_count;
    let mut first = [0u8; 64];
    pool.fill(&mut first).unwrap();

    let mut rest = std::vec![0u8; (count - 1 - 64) as usize];
    pool.fill(&mut rest).unwrap();
    assert_eq!(pool.rs_count, 1);
    assert_eq!(pool.fill(&mut [0u8; 1]), Err(Error::NeedsReseed));

    // Re-seeding with the very same 40 bytes must still change the stream: the seed is
    // XORed into a fresh keystream before the context is re-initialised.
    assert_eq!(pool.stir(SeedSource::GetEntropy, &seed_bytes(0x42)), Ok(()));
    let mut after = [0u8; 64];
    pool.fill(&mut after).unwrap();
    assert_ne!(after, first);
    assert_ne!(after, &rest[..64]);
}

// K523-R04: the fork child starts zeroed/unseeded and does not disturb the parent.
#[test]
fn r04_fork_child_is_unseeded_and_leaves_parent() {
    let seed = seed_bytes(0x77);
    let mut parent = seeded(&seed);
    let mut before = [0u8; 64];
    parent.fill(&mut before).unwrap();

    let mut child = parent.fork_child();
    assert!(!child.is_seeded());
    assert_eq!(child.fill(&mut [0u8; 32]), Err(Error::Unseeded));

    // The parent is exactly where it would be without the fork.
    let mut after = [0u8; 64];
    parent.fill(&mut after).unwrap();
    let mut control = seeded(&seed);
    let mut control_first = [0u8; 64];
    let mut control_second = [0u8; 64];
    control.fill(&mut control_first).unwrap();
    control.fill(&mut control_second).unwrap();
    assert_eq!(before, control_first);
    assert_eq!(after, control_second);
}

// K523-R04: a child reseeded from its own getentropy diverges from the parent keystream.
#[test]
fn r04_fork_child_diverges_after_reseed() {
    let mut parent = seeded(&seed_bytes(0x10));
    let mut parent_before = [0u8; 64];
    parent.fill(&mut parent_before).unwrap();

    let mut child = parent.fork_child();
    child
        .stir(SeedSource::GetEntropy, &seed_bytes(0x20))
        .unwrap();
    let mut child_out = [0u8; 64];
    child.fill(&mut child_out).unwrap();

    let mut parent_after = [0u8; 64];
    parent.fill(&mut parent_after).unwrap();
    assert_ne!(child_out, parent_before);
    assert_ne!(child_out, parent_after);
}

// K523-R05: wipe clears key, IV, keystream, counters and the seeded flag.
#[test]
fn r05_wipe_zeroes_keys_keystream_and_counters() {
    let seed = seed_bytes(0x99);
    let mut pool = seeded(&seed);
    let mut out = [0u8; 32];
    pool.fill(&mut out).unwrap();
    assert!(pool.chacha.key.iter().any(|&byte| byte != 0));
    assert!(pool.rs_buf.iter().any(|&byte| byte != 0));
    assert!(pool.rs_have > 0);

    pool.wipe();
    assert!(!pool.is_seeded());
    assert_eq!(pool.chacha.key, [0u8; KEY_SIZE]);
    assert_eq!(pool.chacha.iv, [0u8; IV_SIZE]);
    assert_eq!(pool.chacha.counter, 0);
    assert_eq!(pool.rs_buf, [0u8; RS_BUFSZ]);
    assert_eq!(pool.rs_have, 0);
    assert_eq!(pool.rs_count, 0);
    assert_eq!(pool.fill(&mut out), Err(Error::Unseeded));

    // A wiped pool restarts exactly like a fresh one, proving the secret state is gone.
    pool.stir(SeedSource::GetEntropy, &seed).unwrap();
    let mut again = [0u8; 64];
    pool.fill(&mut again).unwrap();
    let mut fresh = seeded(&seed);
    let mut expected = [0u8; 64];
    fresh.fill(&mut expected).unwrap();
    assert_eq!(again, expected);

    // Wiping an already-wiped pool is idempotent.
    pool.wipe();
    assert_eq!(pool.chacha.key, [0u8; KEY_SIZE]);
    assert_eq!(pool.rs_buf, [0u8; RS_BUFSZ]);
}

// K523-R06: timer jitter can neither seed nor re-key a live pool.
#[test]
fn r06_timer_jitter_cannot_seed_or_rekey() {
    let seed = seed_bytes(0x5a);
    let mut pool = seeded(&seed);
    let mut first = [0u8; 64];
    pool.fill(&mut first).unwrap();

    assert_eq!(
        pool.stir(SeedSource::TimerJitter, &seed_bytes(0xbb)),
        Err(Error::WeakSource)
    );
    let mut second = [0u8; 64];
    pool.fill(&mut second).unwrap();

    // A control pool with no rejected stir must produce the same continuation.
    let mut control = seeded(&seed);
    let mut control_first = [0u8; 64];
    let mut control_second = [0u8; 64];
    control.fill(&mut control_first).unwrap();
    control.fill(&mut control_second).unwrap();
    assert_eq!(first, control_first);
    assert_eq!(second, control_second);
    assert_ne!(second, first);
}

// K565: the explicit weak software seed uses the same `_rs_stir` construction, while the strict
// `stir(TimerJitter)` path remains rejected above.
#[test]
fn weak_software_stir_uses_the_same_construction() {
    let seed = seed_bytes(0x7e);
    let mut weak = Pool::new();
    assert_eq!(weak.stir_weak(&seed), Ok(()));
    let mut strict = Pool::new();
    assert_eq!(strict.stir(SeedSource::Hardware, &seed), Ok(()));

    let mut weak_stream = [0u8; 128];
    let mut strict_stream = [0u8; 128];
    assert_eq!(weak.fill(&mut weak_stream), Ok(weak_stream.len()));
    assert_eq!(strict.fill(&mut strict_stream), Ok(strict_stream.len()));
    assert_eq!(weak_stream, strict_stream);

    let mut before = [0u8; 64];
    weak.fill(&mut before).unwrap();
    assert_eq!(weak.stir_weak(&seed_bytes(0x7f)), Ok(()));
    let mut after = [0u8; 64];
    weak.fill(&mut after).unwrap();
    assert_ne!(before, after);
}

// The kernel's legacy `add_entropy` shim must be byte-for-byte the explicit `stir` path.
#[test]
fn legacy_add_entropy_matches_stir() {
    let seed = seed_bytes(0x6c);
    let mut legacy = Pool::new();
    legacy.add_entropy(&seed);
    let mut explicit = seeded(&seed);

    let mut a = [0u8; 256];
    let mut b = [0u8; 256];
    assert_eq!(legacy.fill(&mut a), Ok(a.len()));
    assert_eq!(explicit.fill(&mut b), Ok(b.len()));
    assert_eq!(a, b);

    let mut short = Pool::new();
    short.add_entropy(&seed[..SEED_SIZE - 1]);
    assert!(!short.is_seeded());
}
