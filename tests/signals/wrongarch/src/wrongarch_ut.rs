// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! S710b wrong-architecture artifact rejection.

use raven_elf::{Elf, ElfError, Machine};

#[test]
fn foreign_arch_elf_is_rejected_with_bad_machine() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../target/x86_64-unknown-none/release/raven-s44-action"
    );
    let data = std::fs::read(path).expect(
        "x86_64 signal fixture ELF missing; run the signal-action qtest (or its build.sh) first",
    );
    let range = 0x0000_0000_0000_1000..0x0000_7FFF_FFFF_F000;
    let result = Elf::parse(&data, Machine::Aarch64, range);
    assert!(
        matches!(result, Err(ElfError::BadMachine)),
        "x86_64 ELF was not rejected as BadMachine for an aarch64 image"
    );
}
