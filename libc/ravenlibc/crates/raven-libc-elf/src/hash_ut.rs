// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// The independent re-implementation below deliberately uses plain arithmetic; an overflow would
// fail the test itself.

//! Known-answer vectors and an independent re-implementation of both hashes.

use super::{elf_hash, gnu_hash};

/// A second SysV implementation written from the gABI text, using 64-bit arithmetic and an
/// explicit mask, so a copy-paste error in the production function cannot hide.
fn sysv_reference(name: &[u8]) -> u32 {
    let mut hash: u64 = 0;
    for byte in name {
        hash = (hash << 4) + u64::from(*byte);
        let high = (hash >> 24) & 0xf0;
        hash ^= high;
        hash &= 0xffff_ffff;
    }
    (hash & 0x0fff_ffff) as u32
}

/// A second GNU implementation: Horner's rule with an explicit 32-bit mask.
fn gnu_reference(name: &[u8]) -> u32 {
    let mut hash: u64 = 5381;
    for byte in name {
        hash = (hash * 33 + u64::from(*byte)) & 0xffff_ffff;
    }
    hash as u32
}

#[test]
fn the_published_vectors_match() {
    let vectors: [(&[u8], u32, u32); 9] = [
        (b"", 0x0, 0x1505),
        (b"a", 0x61, 0x2b606),
        (b"printf", 0x77905a6, 0x156b2bb8),
        (b"exit", 0x6cf04, 0x7c967e3f),
        (b"dlopen", 0x6b366be, 0xf9040207),
        (b"libc.so.1", 0x865f4e1, 0x5fd7d48e),
        (b"RAVEN_1.0", 0xaa47670, 0xc16c8c2f),
        (b"__tls_get_addr", 0xa36e932, 0x7c8ad2ef),
        (&[0x80, 0xff], 0x8ff, 0x597bc4),
    ];
    for (name, sysv, gnu) in vectors {
        assert_eq!(elf_hash(name), sysv, "elf_hash({name:?})");
        assert_eq!(gnu_hash(name), gnu, "gnu_hash({name:?})");
    }
}

#[test]
fn every_single_byte_name_agrees_with_the_reference() {
    for value in 0..=255u8 {
        let name = [value];
        assert_eq!(elf_hash(&name), sysv_reference(&name), "byte {value:#04x}");
        assert_eq!(gnu_hash(&name), gnu_reference(&name), "byte {value:#04x}");
    }
}

#[test]
fn the_high_bytes_do_not_sign_extend() {
    for value in 0x80..=0xffu8 {
        let name = [value];
        assert_eq!(elf_hash(&name), sysv_reference(&name));
        assert_eq!(gnu_hash(&name), gnu_reference(&name));
    }
    // A signed char would turn 0xff into -1 and change the result.
    assert_ne!(elf_hash(&[0xff]), elf_hash(&[0x7f]));
}

#[test]
fn the_sysv_result_never_sets_the_top_four_bits() {
    let names: [&[u8]; 6] = [
        b"",
        b"a",
        b"printf",
        b"libc.so.1",
        b"__tls_get_addr",
        &[0xff; 32],
    ];
    for name in names {
        assert_eq!(elf_hash(name) & 0xf000_0000, 0, "{name:?}");
    }
}

#[test]
fn a_long_name_hashes_without_panicking() {
    let name = [b'a'; 4096];
    let sysv = elf_hash(&name);
    let gnu = gnu_hash(&name);
    assert_eq!(sysv, sysv_reference(&name));
    assert_eq!(gnu, gnu_reference(&name));
}

#[test]
fn random_names_agree_with_the_reference() {
    let mut state = 0x1234_5678u32;
    for index in 0..2000u32 {
        let length = usize::try_from(index % 37).unwrap();
        let mut name = Vec::new();
        for _ in 0..length {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            name.push(u8::try_from(state & 0xff).unwrap());
        }
        assert_eq!(elf_hash(&name), sysv_reference(&name), "run {index}");
        assert_eq!(gnu_hash(&name), gnu_reference(&name), "run {index}");
    }
}

#[test]
fn the_empty_name_has_the_djb_seed() {
    assert_eq!(gnu_hash(b""), 5381);
    assert_eq!(elf_hash(b""), 0);
}

#[test]
fn the_hashes_are_stable_across_calls() {
    let name = b"libc.so.1";
    assert_eq!(elf_hash(name), elf_hash(name));
    assert_eq!(gnu_hash(name), gnu_hash(name));
}
