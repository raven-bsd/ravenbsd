// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The syscall tables match `linux-raw-sys` on every supported architecture.

mod support;

use raven_linux_abi::{Arch, sysnums};
use std::collections::BTreeSet;

fn table(arch: Arch) -> &'static [(&'static str, u32)] {
    match arch {
        Arch::X86_64 => sysnums::X86_64,
        Arch::Aarch64 => sysnums::AARCH64,
        Arch::Riscv64 => sysnums::RISCV64,
    }
}

fn assert_matches_linux_raw_sys(arch: Arch, relative: &str) {
    let ours = table(arch);
    let theirs = support::syscalls(relative);
    assert_eq!(
        ours.len(),
        theirs.len(),
        "{}: table length differs from {}",
        arch.name(),
        relative
    );
    for (name, number) in ours {
        assert_eq!(
            theirs.get(*name),
            Some(number),
            "{}: {} differs from linux-raw-sys",
            arch.name(),
            name
        );
    }
}

#[test]
fn x86_64_matches_linux_raw_sys() {
    assert_matches_linux_raw_sys(Arch::X86_64, "src/x86_64/general.rs");
}

#[test]
fn aarch64_matches_linux_raw_sys() {
    assert_matches_linux_raw_sys(Arch::Aarch64, "src/aarch64/general.rs");
}

#[test]
fn riscv64_matches_linux_raw_sys() {
    assert_matches_linux_raw_sys(Arch::Riscv64, "src/riscv64/general.rs");
}

#[test]
fn x86_64_spot_values() {
    assert_eq!(sysnums::number(Arch::X86_64, "read"), Some(0));
    assert_eq!(sysnums::number(Arch::X86_64, "write"), Some(1));
    assert_eq!(sysnums::number(Arch::X86_64, "openat"), Some(257));
    assert_eq!(sysnums::number(Arch::X86_64, "mmap"), Some(9));
    assert_eq!(sysnums::number(Arch::X86_64, "futex"), Some(202));
    assert_eq!(sysnums::number(Arch::X86_64, "clone"), Some(56));
    assert_eq!(sysnums::number(Arch::X86_64, "exit_group"), Some(231));
}

#[test]
fn aarch64_spot_values() {
    assert_eq!(sysnums::number(Arch::Aarch64, "read"), Some(63));
    assert_eq!(sysnums::number(Arch::Aarch64, "write"), Some(64));
    assert_eq!(sysnums::number(Arch::Aarch64, "openat"), Some(56));
    assert_eq!(sysnums::number(Arch::Aarch64, "mmap"), Some(222));
    assert_eq!(sysnums::number(Arch::Aarch64, "futex"), Some(98));
    assert_eq!(sysnums::number(Arch::Aarch64, "clone"), Some(220));
    assert_eq!(sysnums::number(Arch::Aarch64, "exit_group"), Some(94));
}

#[test]
fn riscv64_spot_values() {
    assert_eq!(sysnums::number(Arch::Riscv64, "read"), Some(63));
    assert_eq!(sysnums::number(Arch::Riscv64, "write"), Some(64));
    assert_eq!(sysnums::number(Arch::Riscv64, "openat"), Some(56));
    assert_eq!(sysnums::number(Arch::Riscv64, "mmap"), Some(222));
    assert_eq!(sysnums::number(Arch::Riscv64, "futex"), Some(98));
    assert_eq!(sysnums::number(Arch::Riscv64, "clone"), Some(220));
    assert_eq!(sysnums::number(Arch::Riscv64, "exit_group"), Some(94));
}

#[test]
fn tables_are_sorted_by_name() {
    for arch in Arch::ALL {
        let table = table(arch);
        assert!(
            table.windows(2).all(|pair| pair[0].0 < pair[1].0),
            "{} table is not strictly sorted by name",
            arch.name()
        );
    }
}

#[test]
fn tables_have_no_duplicate_numbers() {
    for arch in Arch::ALL {
        let mut numbers: Vec<u32> = table(arch).iter().map(|(_, number)| *number).collect();
        numbers.sort_unstable();
        let before = numbers.len();
        numbers.dedup();
        assert_eq!(
            before,
            numbers.len(),
            "{} has duplicate numbers",
            arch.name()
        );
    }
}

#[test]
fn number_and_name_round_trip() {
    for arch in Arch::ALL {
        for &(name, number) in table(arch) {
            assert_eq!(sysnums::number(arch, name), Some(number), "{}", arch.name());
            assert_eq!(sysnums::name(arch, number), Some(name), "{}", arch.name());
        }
    }
}

#[test]
fn unknown_lookups_return_none() {
    assert_eq!(sysnums::number(Arch::X86_64, "not_a_syscall"), None);
    assert_eq!(sysnums::name(Arch::X86_64, u32::MAX), None);
    assert_eq!(sysnums::number(Arch::Aarch64, "arch_prctl"), None);
    assert_eq!(sysnums::number(Arch::Riscv64, "arch_prctl"), None);
}

#[test]
fn all_names_is_the_sorted_union() {
    let mut union = BTreeSet::new();
    for arch in Arch::ALL {
        union.extend(table(arch).iter().map(|(name, _)| *name));
    }
    let expected: Vec<&str> = union.into_iter().collect();
    assert_eq!(sysnums::ALL_NAMES, expected);
}

#[test]
fn all_names_is_sorted_and_unique() {
    assert!(sysnums::ALL_NAMES.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn arch_names_are_stable() {
    assert_eq!(Arch::X86_64.name(), "x86_64");
    assert_eq!(Arch::Aarch64.name(), "aarch64");
    assert_eq!(Arch::Riscv64.name(), "riscv64");
    assert_eq!(Arch::ALL.len(), 3);
}
