// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

fn input_for(arch: u8, bias: u8, image: &[u8], names: &[u8]) -> Vec<u8> {
    let length = u16::try_from(image.len()).unwrap_or(u16::MAX);
    let mut input = vec![arch, bias];
    input.extend_from_slice(&length.to_le_bytes());
    input.extend_from_slice(image);
    input.extend_from_slice(names);
    input
}

#[test]
fn elf_dynamic_hostile_ok() {
    body(&input_for(0, 0, &[], b""));
    body(&input_for(0, 0, &[0u8; 4096], b"foo"));
}

#[test]
fn elf_dynamic_boundaries_ok() {
    let mut header = vec![0x7f, b'E', b'L', b'F', 2, 1, 1, 0];
    header.extend_from_slice(&[0u8; 8]);
    header.extend_from_slice(&2u16.to_le_bytes());
    header.extend_from_slice(&62u16.to_le_bytes());
    header.extend_from_slice(&1u32.to_le_bytes());
    header.extend_from_slice(&0u64.to_le_bytes());
    header.extend_from_slice(&64u64.to_le_bytes());
    header.extend_from_slice(&0u64.to_le_bytes());
    header.extend_from_slice(&64u16.to_le_bytes());
    header.extend_from_slice(&56u16.to_le_bytes());
    header.extend_from_slice(&65535u16.to_le_bytes());
    header.extend_from_slice(&[0u8; 6]);
    body(&input_for(0, 0, &header, b""));
    body(&input_for(1, 1, &[0xff; 4096], b""));
}

#[test]
#[should_panic(expected = "P-elf-6")]
fn elf_dynamic_mutation_6_fires() {
    let order = [1u32, 0];
    let finish = [1u32, 0];
    let deps = |id: u32| if id == 1 { vec![0] } else { Vec::new() };
    assert!(
        model::init_order_ok(&order, &finish, &deps),
        "P-elf-6: init_order is not dependency-first or fini is not its reverse"
    );
}

#[test]
fn every_architecture_and_bias_runs() {
    for arch in 0u8..3 {
        for bias in 0u8..2 {
            body(&input_for(arch, bias, &[0u8; 64], b"foo\0bar\0"));
        }
    }
}

#[test]
fn hash_models_agree_with_the_crate() {
    for name in [b"".as_slice(), b"printf", b"main", b"_start"] {
        assert_eq!(elf_hash(name), model::elf_hash(name));
        assert_eq!(gnu_hash(name), model::gnu_hash(name));
    }
}
