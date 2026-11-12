// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Unit tests for the fuzz targets and their helpers.

use super::*;

#[test]
fn every_target_has_a_stable_name() {
    for target in Target::ALL {
        assert_eq!(Target::from_name(target.name()), Some(target));
    }
    assert_eq!(Target::from_name("none"), None);
}

#[test]
fn every_seed_runs() {
    for target in Target::ALL {
        run(target, &seed(target));
    }
}

#[test]
fn every_seed_runs_with_a_short_fuzz_sweep() {
    for target in Target::ALL {
        let executed = fuzz(target, 7, 200);
        assert_eq!(executed, 200, "{}", target.name());
    }
}

#[test]
fn the_sweep_is_deterministic() {
    let first: Vec<Vec<u8>> = {
        let mut rng = Rng::new(42);
        (0..16).map(|_| mutate(b"seed", &mut rng)).collect()
    };
    let second: Vec<Vec<u8>> = {
        let mut rng = Rng::new(42);
        (0..16).map(|_| mutate(b"seed", &mut rng)).collect()
    };
    assert_eq!(first, second);
}

#[test]
fn the_rng_never_returns_out_of_range() {
    let mut rng = Rng::new(1);
    for _ in 0..1000 {
        assert!(rng.below(7) < 7);
    }
    let mut rng = Rng::new(0);
    assert_ne!(rng.next_u64(), 0);
}

#[test]
fn a_mutated_input_is_never_empty() {
    let mut rng = Rng::new(3);
    for _ in 0..100 {
        let input = mutate(&[], &mut rng);
        assert!(!input.is_empty());
    }
}

#[test]
fn the_elf_seed_parses_and_plans() {
    let image = seed(Target::Elf);
    let parsed = Elf::parse(&image, Machine::X86_64, USER_START..USER_END).expect("valid ELF");
    let steps: Vec<_> = plan_load(&parsed, 4096, LoadMode::Strict)
        .expect("valid plan")
        .collect();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].vaddr, 0x40_0000);
    assert_eq!(steps[0].len, 4096);
}

#[test]
fn the_ipc_seed_decodes() {
    let bytes = seed(Target::Ipc);
    let packet = protocol::decode(&bytes).expect("valid packet");
    assert_eq!(packet.opcode, Opcode::Open.as_u16());
    assert_eq!(packet.path_bytes(), b"/bin/cat");
}

#[test]
fn the_syscall_seed_matches_known_numbers() {
    let bytes = seed(Target::Syscall);
    let numbers: Vec<u64> = bytes
        .chunks(8)
        .map(|chunk| u64::from_le_bytes(chunk.try_into().expect("eight bytes")))
        .collect();
    assert_eq!(
        Syscall::from_usize(numbers[0] as usize),
        Some(Syscall::EXIT)
    );
    assert_eq!(
        Syscall::from_usize(numbers[1] as usize),
        Some(Syscall::DEBUG_READ)
    );
    assert!(Syscall::from_usize(numbers[3] as usize).is_none());
}

#[test]
fn hex_round_trips() {
    let bytes: Vec<u8> = (0u8..=255).collect();
    let text = to_hex(&bytes);
    assert_eq!(from_hex(&text).unwrap(), bytes);
}

#[test]
fn hex_ignores_comments_and_whitespace() {
    let decoded = from_hex("# a comment\n7f 45\n4c 46 # trailing\n").unwrap();
    assert_eq!(decoded, b"\x7fELF");
}

#[test]
fn hex_rejects_bad_digits_and_odd_lengths() {
    assert_eq!(from_hex("zz"), Err(HexError::BadDigit));
    assert_eq!(from_hex("abc"), Err(HexError::OddLength));
}

#[test]
fn frames_are_accepted_only_inside_the_checked_range() {
    assert!(validate_range(USER_START, 1, USER_START, USER_END).is_ok());
    assert!(validate_range(USER_START, u64::MAX, USER_START, USER_END).is_err());
}

#[test]
fn the_targets_survive_the_seed_with_a_huge_mutation_budget() {
    // A larger budget than the tool uses by default; keeps the shipped test fast.
    for target in Target::ALL {
        let executed = fuzz(target, 0x5eed, 2_000);
        assert_eq!(executed, 2_000, "{}", target.name());
    }
}
