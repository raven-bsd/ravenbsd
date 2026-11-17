// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Byte-builder tests for `nm` filtering, type letters and rendering.

use super::*;
use raven_elf::inspect::{
    SHN_ABS, SHN_COMMON, SHT_PROGBITS, SHT_STRTAB, SHT_SYMTAB, STB_GLOBAL, STT_FUNC, STT_NOTYPE,
    STT_OBJECT,
};

fn put16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

const SHOFF: usize = 0x600;
const SYM_OFF: usize = 0x400;
const SHSTR_OFF: usize = 0x200;
const STR_OFF: usize = 0x300;
const NAMES: &[u8] = b"\0.shstrtab\0.text\0.data\0.bss\0.symtab\0.strtab\0";
const STRINGS: &[u8] =
    b"\0_start\0data_obj\0undef_fn\0weak_fn\0abs_sym\0common_obj\0local_fn\0file.c\0weak_obj\0";

fn symbol64(data: &mut [u8], index: usize, name: u32, info: u8, shndx: u16, value: u64, size: u64) {
    let base = SYM_OFF + index * 24;
    put32(data, base, name);
    data[base + 4] = info;
    data[base + 5] = 0;
    put16(data, base + 6, shndx);
    put64(data, base + 8, value);
    put64(data, base + 16, size);
}

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

/// Build an ELF64 executable with text, data, bss, absolute, common and weak symbols.
fn elf64() -> Vec<u8> {
    let mut data = vec![0u8; SHOFF + 7 * 64];
    data[0..4].copy_from_slice(b"\x7fELF");
    data[4] = 2;
    data[5] = 1;
    data[6] = 1;
    put16(&mut data, 16, 2); // ET_EXEC
    put16(&mut data, 18, 62); // x86-64
    put32(&mut data, 20, 1);
    put64(&mut data, 24, 0x401000);
    put64(&mut data, 40, SHOFF as u64);
    put16(&mut data, 52, 64);
    put16(&mut data, 58, 64);
    put16(&mut data, 60, 7);
    put16(&mut data, 62, 1);

    data[SHSTR_OFF..SHSTR_OFF + NAMES.len()].copy_from_slice(NAMES);
    data[STR_OFF..STR_OFF + STRINGS.len()].copy_from_slice(STRINGS);

    section64(
        &mut data,
        1,
        1,
        SHT_STRTAB,
        0,
        0,
        0x200,
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
        0x20,
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
        0x3,
        0x402000,
        0x260,
        0x10,
        0,
        0,
        8,
        0,
    );
    section64(
        &mut data, 4, 23, SHT_NOBITS, 0x3, 0x403000, 0x270, 0x10, 0, 0, 8, 0,
    );
    section64(
        &mut data,
        5,
        28,
        SHT_SYMTAB,
        0,
        0,
        SYM_OFF as u64,
        10 * 24,
        6,
        1,
        8,
        24,
    );
    section64(
        &mut data,
        6,
        36,
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

    symbol64(
        &mut data,
        1,
        1,
        (STB_GLOBAL << 4) | STT_FUNC,
        2,
        0x401000,
        16,
    );
    symbol64(
        &mut data,
        2,
        8,
        (STB_GLOBAL << 4) | STT_OBJECT,
        3,
        0x402000,
        8,
    );
    symbol64(
        &mut data,
        3,
        17,
        (STB_GLOBAL << 4) | STT_FUNC,
        SHN_UNDEF,
        0,
        0,
    );
    symbol64(&mut data, 4, 26, (STB_WEAK << 4) | STT_FUNC, 2, 0x401010, 4);
    symbol64(
        &mut data,
        5,
        34,
        (STB_GLOBAL << 4) | STT_NOTYPE,
        SHN_ABS,
        0x1234,
        0,
    );
    symbol64(
        &mut data,
        6,
        42,
        (STB_GLOBAL << 4) | STT_OBJECT,
        SHN_COMMON,
        8,
        4,
    );
    symbol64(&mut data, 7, 53, STT_FUNC, 2, 0x401014, 4);
    symbol64(&mut data, 8, 62, STT_FILE, SHN_ABS, 0, 0);
    symbol64(
        &mut data,
        9,
        69,
        (STB_WEAK << 4) | STT_OBJECT,
        3,
        0x402010,
        8,
    );
    data
}

/// Build a small ELF32 relocatable object with one text symbol.
fn elf32() -> Vec<u8> {
    let names: &[u8] = b"\0.shstrtab\0.text\0.symtab\0.strtab\0";
    let strings: &[u8] = b"\0foo\0";
    let shoff = 0x300usize;
    let mut data = vec![0u8; shoff + 5 * 40];
    data[0..4].copy_from_slice(b"\x7fELF");
    data[4] = 1;
    data[5] = 1;
    data[6] = 1;
    put16(&mut data, 16, 1);
    put16(&mut data, 18, 3);
    put32(&mut data, 20, 1);
    put32(&mut data, 32, shoff as u32);
    put16(&mut data, 40, 52);
    put16(&mut data, 46, 40);
    put16(&mut data, 48, 5);
    put16(&mut data, 50, 1);
    data[0x100..0x100 + names.len()].copy_from_slice(names);
    data[0x180..0x180 + strings.len()].copy_from_slice(strings);

    let sh = |index: usize| shoff + index * 40;
    let mut base = sh(1);
    put32(&mut data, base, 1);
    put32(&mut data, base + 4, SHT_STRTAB);
    put32(&mut data, base + 16, 0x100);
    put32(&mut data, base + 20, names.len() as u32);
    put32(&mut data, base + 32, 1);

    base = sh(2);
    put32(&mut data, base, 11);
    put32(&mut data, base + 4, SHT_PROGBITS);
    put32(&mut data, base + 8, 0x6);
    put32(&mut data, base + 12, 0x1000);
    put32(&mut data, base + 16, 0x140);
    put32(&mut data, base + 20, 8);
    put32(&mut data, base + 32, 4);

    base = sh(3);
    put32(&mut data, base, 17);
    put32(&mut data, base + 4, SHT_SYMTAB);
    put32(&mut data, base + 16, 0x200);
    put32(&mut data, base + 20, 2 * 16);
    put32(&mut data, base + 24, 4);
    put32(&mut data, base + 28, 1);
    put32(&mut data, base + 32, 4);
    put32(&mut data, base + 36, 16);

    base = sh(4);
    put32(&mut data, base, 25);
    put32(&mut data, base + 4, SHT_STRTAB);
    put32(&mut data, base + 16, 0x180);
    put32(&mut data, base + 20, strings.len() as u32);
    put32(&mut data, base + 32, 1);

    let sym = 0x200 + 16;
    put32(&mut data, sym, 1);
    put32(&mut data, sym + 4, 0x1000);
    put32(&mut data, sym + 8, 8);
    data[sym + 12] = (STB_GLOBAL << 4) | STT_FUNC;
    put16(&mut data, sym + 14, 2);
    data
}

fn names(entries: &[Entry]) -> Vec<(String, char)> {
    entries
        .iter()
        .map(|entry| (entry.name.clone(), entry.kind))
        .collect()
}

fn run(data: &[u8], args: &[&str]) -> (Class, Vec<Entry>) {
    let options = parse_args(args.iter().copied()).expect("options");
    inspect(data, &options).expect("inspect")
}

#[test]
fn default_lists_locals_and_skips_file_symbols() {
    let data = elf64();
    let (class, entries) = run(&data, &["file"]);
    assert_eq!(class, Class::Elf64);
    assert_eq!(
        names(&entries),
        vec![
            ("_start".to_string(), 'T'),
            ("abs_sym".to_string(), 'A'),
            ("common_obj".to_string(), 'C'),
            ("data_obj".to_string(), 'D'),
            ("local_fn".to_string(), 't'),
            ("undef_fn".to_string(), 'U'),
            ("weak_fn".to_string(), 'W'),
            ("weak_obj".to_string(), 'V'),
        ]
    );
    assert!(
        entries
            .iter()
            .find(|entry| entry.name == "undef_fn")
            .unwrap()
            .undefined
    );
}

#[test]
fn all_includes_the_file_symbol() {
    let data = elf64();
    let (_, entries) = run(&data, &["-a", "file"]);
    assert!(names(&entries).contains(&("file.c".to_string(), 'a')));
}

#[test]
fn global_filter_drops_locals() {
    let data = elf64();
    let (_, entries) = run(&data, &["-g", "file"]);
    assert_eq!(entries.len(), 7);
    assert!(!entries.iter().any(|entry| entry.name == "local_fn"));
    assert!(!entries.iter().any(|entry| entry.name == "file.c"));
}

#[test]
fn undefined_filter_keeps_only_undefined_symbols() {
    let data = elf64();
    let (_, entries) = run(&data, &["-u", "file"]);
    assert_eq!(names(&entries), vec![("undef_fn".to_string(), 'U')]);
}

#[test]
fn numeric_sort_uses_value_then_name() {
    let data = elf64();
    let (_, entries) = run(&data, &["-n", "file"]);
    let ordered: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(
        ordered,
        vec![
            "undef_fn",
            "common_obj",
            "abs_sym",
            "_start",
            "weak_fn",
            "local_fn",
            "data_obj",
            "weak_obj",
        ]
    );
}

#[test]
fn preserve_order_uses_the_symbol_table() {
    let data = elf64();
    let (_, entries) = run(&data, &["-p", "file"]);
    let ordered: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(
        ordered,
        vec![
            "_start",
            "data_obj",
            "undef_fn",
            "weak_fn",
            "abs_sym",
            "common_obj",
            "local_fn",
            "weak_obj",
        ]
    );
}

#[test]
fn reverse_reverses_the_selected_order() {
    let data = elf64();
    let (_, entries) = run(&data, &["-r", "file"]);
    let ordered: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(
        ordered,
        vec![
            "weak_obj",
            "weak_fn",
            "undef_fn",
            "local_fn",
            "data_obj",
            "common_obj",
            "abs_sym",
            "_start",
        ]
    );
}

#[test]
fn elf32_symbols_use_eight_hex_digits() {
    let data = elf32();
    let (class, entries) = run(&data, &["file"]);
    assert_eq!(class, Class::Elf32);
    assert_eq!(names(&entries), vec![("foo".to_string(), 'T')]);
    let lines = render(&entries, class);
    assert_eq!(lines, vec!["00001000 T foo".to_string()]);
}

#[test]
fn undefined_values_render_as_blanks() {
    let data = elf64();
    let (class, entries) = run(&data, &["-u", "file"]);
    let lines = render(&entries, class);
    assert_eq!(lines, vec!["                 U undef_fn".to_string()]);
}

#[test]
fn malformed_input_is_rejected() {
    let options = parse_args(["file"]).expect("options");
    assert_eq!(inspect(b"not an elf", &options), Err(NmError::Malformed));
}

#[test]
fn argument_parsing_bundles_options_and_stops_at_double_dash() {
    let options = parse_args(["-gnur", "--", "-a"]).expect("options");
    assert!(options.external_only);
    assert!(options.numeric);
    assert!(options.undefined_only);
    assert!(options.reverse);
    assert!(!options.all);
    assert_eq!(options.operands, vec!["-a"]);
}

#[test]
fn missing_operand_and_unknown_option_are_usage_errors() {
    assert_eq!(
        parse_args(std::iter::empty::<&str>()),
        Err(UsageError::MissingOperand)
    );
    assert_eq!(
        parse_args(["-z", "file"]),
        Err(UsageError::UnknownOption('z'))
    );
}
