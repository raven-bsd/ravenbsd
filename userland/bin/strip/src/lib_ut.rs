// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Byte-builder tests for `strip` header neutralization and option parsing.

use super::*;
use raven_elf::inspect::{SHT_PROGBITS, STB_GLOBAL, STT_FUNC};

fn put16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

const SHOFF: usize = 0x500;
const SYM_OFF: usize = 0x380;
const SHSTR_OFF: usize = 0x200;
const STR_OFF: usize = 0x300;
const DEBUG_OFF: usize = 0x400;
const NAMES: &[u8] = b"\0.shstrtab\0.text\0.debug_info\0.symtab\0.strtab\0";
const STRINGS: &[u8] = b"\0foo\0";

#[allow(clippy::too_many_arguments)] // Byte-builder mirroring the ELF64 section header.
fn section64(
    data: &mut [u8],
    index: usize,
    name: u32,
    kind: u32,
    flags: u64,
    address: u64,
    offset: u64,
    size: u64,
    link: u32,
    info: u32,
    align: u64,
    entsize: u64,
) {
    let base = SHOFF + index * 64;
    put32(data, base, name);
    put32(data, base + 4, kind);
    put64(data, base + 8, flags);
    put64(data, base + 16, address);
    put64(data, base + 24, offset);
    put64(data, base + 32, size);
    put32(data, base + 40, link);
    put32(data, base + 44, info);
    put64(data, base + 48, align);
    put64(data, base + 56, entsize);
}

/// Build an ELF64 executable with text, debug, symtab and strtab sections.
fn elf64() -> Vec<u8> {
    let mut data = vec![0u8; SHOFF + 6 * 64];
    data[0..4].copy_from_slice(b"\x7fELF");
    data[4] = 2;
    data[5] = 1;
    data[6] = 1;
    put16(&mut data, 16, 2); // ET_EXEC
    put16(&mut data, 18, 62);
    put32(&mut data, 20, 1);
    put64(&mut data, 24, 0x401000);
    put64(&mut data, 40, SHOFF as u64);
    put16(&mut data, 52, 64);
    put16(&mut data, 58, 64);
    put16(&mut data, 60, 6);
    put16(&mut data, 62, 1);

    data[SHSTR_OFF..SHSTR_OFF + NAMES.len()].copy_from_slice(NAMES);
    data[STR_OFF..STR_OFF + STRINGS.len()].copy_from_slice(STRINGS);
    data[0x240..0x248].copy_from_slice(&[0x90; 8]);
    data[DEBUG_OFF..DEBUG_OFF + 16].copy_from_slice(&[0x11; 16]);

    section64(
        &mut data,
        1,
        1,
        SHT_STRTAB,
        0,
        0,
        SHSTR_OFF as u64,
        NAMES.len() as u64,
        0,
        0,
        1,
        0,
    );
    section64(
        &mut data,
        2,
        11,
        SHT_PROGBITS,
        0x6,
        0x401000,
        0x240,
        8,
        0,
        0,
        16,
        0,
    );
    section64(
        &mut data,
        3,
        17,
        SHT_PROGBITS,
        0,
        0,
        DEBUG_OFF as u64,
        16,
        0,
        0,
        1,
        0,
    );
    section64(
        &mut data,
        4,
        29,
        SHT_SYMTAB,
        0,
        0,
        SYM_OFF as u64,
        2 * 24,
        5,
        1,
        8,
        24,
    );
    section64(
        &mut data,
        5,
        37,
        SHT_STRTAB,
        0,
        0,
        STR_OFF as u64,
        STRINGS.len() as u64,
        0,
        0,
        1,
        0,
    );

    let symbol = SYM_OFF + 24;
    put32(&mut data, symbol, 1);
    data[symbol + 4] = (STB_GLOBAL << 4) | STT_FUNC;
    put16(&mut data, symbol + 6, 2);
    put64(&mut data, symbol + 8, 0x401000);
    put64(&mut data, symbol + 16, 8);
    data
}

/// Build a minimal ELF32 executable with a debug section.
fn elf32() -> Vec<u8> {
    let names: &[u8] = b"\0.shstrtab\0.text\0.debug_info\0";
    let shoff = 0x200usize;
    let mut data = vec![0u8; shoff + 4 * 40];
    data[0..4].copy_from_slice(b"\x7fELF");
    data[4] = 1;
    data[5] = 1;
    data[6] = 1;
    put16(&mut data, 16, 2);
    put16(&mut data, 18, 3);
    put32(&mut data, 20, 1);
    put32(&mut data, 32, shoff as u32);
    put16(&mut data, 40, 52);
    put16(&mut data, 46, 40);
    put16(&mut data, 48, 4);
    put16(&mut data, 50, 1);
    data[0x100..0x100 + names.len()].copy_from_slice(names);

    let mut base = shoff + 40;
    put32(&mut data, base, 1);
    put32(&mut data, base + 4, SHT_STRTAB);
    put32(&mut data, base + 16, 0x100);
    put32(&mut data, base + 20, names.len() as u32);
    put32(&mut data, base + 32, 1);

    base = shoff + 80;
    put32(&mut data, base, 11);
    put32(&mut data, base + 4, SHT_PROGBITS);
    put32(&mut data, base + 8, 0x6);
    put32(&mut data, base + 12, 0x1000);
    put32(&mut data, base + 16, 0x140);
    put32(&mut data, base + 20, 4);
    put32(&mut data, base + 32, 4);

    base = shoff + 120;
    put32(&mut data, base, 17);
    put32(&mut data, base + 4, SHT_PROGBITS);
    put32(&mut data, base + 16, 0x150);
    put32(&mut data, base + 20, 4);
    put32(&mut data, base + 32, 1);
    data
}

#[test]
fn all_removes_symbols_string_table_and_debug_sections() {
    let mut data = elf64();
    let before = data.clone();
    let report = strip(&mut data, Mode::All).expect("strip");
    assert_eq!(
        report,
        Report {
            class: Class::Elf64,
            blanked: 3
        }
    );
    let object = Object::parse(&data).expect("reparses");
    assert_eq!(object.section_count(), 6);
    assert_eq!(object.object_type(), 2);
    assert_eq!(object.section(2).unwrap().section_type(), SHT_PROGBITS);
    assert_eq!(object.section(2).unwrap().data(), &[0x90; 8]);
    assert_eq!(object.section(1).unwrap().name(), Some(".shstrtab"));
    assert_eq!(object.section(2).unwrap().name(), Some(".text"));
    assert_eq!(object.section(3).unwrap().section_type(), 0);
    assert_eq!(object.section(4).unwrap().section_type(), 0);
    assert_eq!(object.section(5).unwrap().section_type(), 0);
    assert_eq!(object.symbols().count(), 0);
    // Program/section data outside the neutralized headers is untouched.
    assert_eq!(&data[0x240..0x248], &[0x90; 8]);
    assert_eq!(&data[DEBUG_OFF..DEBUG_OFF + 16], &[0x11; 16]);
    // The ELF header itself is unchanged; only the section headers were neutralized.
    assert_eq!(&data[0..64], &before[0..64]);
    assert_ne!(
        &data[SHOFF + 4 * 64..SHOFF + 5 * 64],
        &before[SHOFF + 4 * 64..SHOFF + 5 * 64]
    );
}

#[test]
fn debug_mode_keeps_the_symbol_table() {
    let mut data = elf64();
    let report = strip(&mut data, Mode::Debug).expect("strip");
    assert_eq!(report.blanked, 1);
    let object = Object::parse(&data).expect("reparses");
    assert_eq!(object.section(3).unwrap().section_type(), 0);
    assert_eq!(object.section(4).unwrap().section_type(), SHT_SYMTAB);
    assert_eq!(object.section(5).unwrap().section_type(), SHT_STRTAB);
    let symbols: Vec<_> = object.symbols().collect();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[1].name(), Some("foo"));
}

#[test]
fn stripping_twice_is_idempotent() {
    let mut data = elf64();
    assert_eq!(strip(&mut data, Mode::All).expect("first").blanked, 3);
    assert_eq!(strip(&mut data, Mode::All).expect("second").blanked, 0);
    assert!(Object::parse(&data).is_ok());
}

#[test]
fn elf32_debug_sections_are_neutralized() {
    let mut data = elf32();
    let report = strip(&mut data, Mode::All).expect("strip");
    assert_eq!(
        report,
        Report {
            class: Class::Elf32,
            blanked: 1
        }
    );
    let object = Object::parse(&data).expect("reparses");
    assert_eq!(object.class(), Class::Elf32);
    assert_eq!(object.section(2).unwrap().name(), Some(".text"));
    assert_eq!(object.section(3).unwrap().section_type(), 0);
}

#[test]
fn malformed_input_is_rejected() {
    let mut data = b"not an elf".to_vec();
    assert_eq!(strip(&mut data, Mode::All), Err(StripError::Malformed));
}

#[test]
fn argument_parsing_accepts_g_s_and_separate_output() {
    let options = parse_args(["-g", "-o", "out", "file"]).expect("options");
    assert_eq!(options.mode, Mode::Debug);
    assert_eq!(options.output, Some("out"));
    assert_eq!(options.operands, vec!["file"]);

    let options = parse_args(["-soout", "file"]).expect("options");
    assert_eq!(options.mode, Mode::All);
    assert_eq!(options.output, Some("out"));
}

#[test]
fn argument_errors_are_reported() {
    assert_eq!(
        parse_args(std::iter::empty::<&str>()),
        Err(UsageError::MissingOperand)
    );
    assert_eq!(parse_args(["-o"]), Err(UsageError::MissingOperand));
    assert_eq!(
        parse_args(["-z", "file"]),
        Err(UsageError::UnknownOption('z'))
    );
    assert_eq!(
        parse_args(["-o", "out", "a", "b"]),
        Err(UsageError::OutputWithMultipleFiles)
    );
}
