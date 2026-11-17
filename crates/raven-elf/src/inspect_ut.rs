// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Byte-builder tests for the bounded section/symbol inspection API.

use super::*;
use std::vec;
use std::vec::Vec;

fn put16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn names() -> Vec<u8> {
    let mut table = Vec::new();
    table.push(0);
    table.extend_from_slice(b".shstrtab\0.text\0.symtab\0.strtab\0");
    table
}

fn strings() -> Vec<u8> {
    let mut table = Vec::new();
    table.push(0);
    table.extend_from_slice(b"foo\0bar\0");
    table
}

/// Build an ELF64 relocatable object with .shstrtab, .text, .symtab and .strtab.
fn elf64() -> Vec<u8> {
    let mut data = vec![0u8; 0x400];
    data[0..4].copy_from_slice(b"\x7fELF");
    data[4] = 2;
    data[5] = 1;
    data[6] = 1;
    put16(&mut data, 16, 1); // ET_REL
    put16(&mut data, 18, 62); // x86-64
    put32(&mut data, 20, 1);
    put64(&mut data, 40, 0x40); // e_shoff
    put16(&mut data, 52, 64);
    put16(&mut data, 58, 64); // e_shentsize
    put16(&mut data, 60, 5); // e_shnum
    put16(&mut data, 62, 1); // e_shstrndx

    let shstr = names();
    data[0x200..0x200 + shstr.len()].copy_from_slice(&shstr);
    data[0x240..0x248].copy_from_slice(&[0x90; 8]);
    let strtab = strings();
    data[0x300..0x300 + strtab.len()].copy_from_slice(&strtab);

    // Section 1: .shstrtab.
    let base = 0x40 + 64;
    put32(&mut data, base, 1);
    put32(&mut data, base + 4, SHT_STRTAB);
    put64(&mut data, base + 24, 0x200);
    put64(&mut data, base + 32, shstr.len() as u64);
    put64(&mut data, base + 48, 1);

    // Section 2: .text.
    let base = 0x40 + 2 * 64;
    put32(&mut data, base, 11);
    put32(&mut data, base + 4, 1);
    put64(&mut data, base + 8, 0x6);
    put64(&mut data, base + 16, 0x1000);
    put64(&mut data, base + 24, 0x240);
    put64(&mut data, base + 32, 8);
    put64(&mut data, base + 48, 16);

    // Section 3: .symtab (three entries: null, foo, bar).
    let base = 0x40 + 3 * 64;
    put32(&mut data, base, 17);
    put32(&mut data, base + 4, SHT_SYMTAB);
    put64(&mut data, base + 24, 0x280);
    put64(&mut data, base + 32, 3 * 24);
    put32(&mut data, base + 40, 4);
    put32(&mut data, base + 44, 1);
    put64(&mut data, base + 48, 8);
    put64(&mut data, base + 56, 24);

    // Section 4: .strtab.
    let base = 0x40 + 4 * 64;
    put32(&mut data, base, 25);
    put32(&mut data, base + 4, SHT_STRTAB);
    put64(&mut data, base + 24, 0x300);
    put64(&mut data, base + 32, strtab.len() as u64);
    put64(&mut data, base + 48, 1);

    // Symbol 1: foo, global function, .text.
    let sym = 0x280 + 24;
    put32(&mut data, sym, 1);
    data[sym + 4] = (STB_GLOBAL << 4) | STT_FUNC;
    put16(&mut data, sym + 6, 2);
    put64(&mut data, sym + 8, 0x1000);
    put64(&mut data, sym + 16, 8);

    // Symbol 2: bar, local object, undefined.
    let sym = 0x280 + 2 * 24;
    put32(&mut data, sym, 5);
    data[sym + 4] = (STB_LOCAL << 4) | STT_OBJECT;
    put16(&mut data, sym + 6, SHN_UNDEF);
    data
}

/// Build an ELF32 relocatable object with the same four meaningful sections.
fn elf32() -> Vec<u8> {
    let mut data = vec![0u8; 0x400];
    data[0..4].copy_from_slice(b"\x7fELF");
    data[4] = 1;
    data[5] = 1;
    data[6] = 1;
    put16(&mut data, 16, 1);
    put16(&mut data, 18, 3); // EM_386
    put32(&mut data, 20, 1);
    put32(&mut data, 32, 0x40); // e_shoff
    put16(&mut data, 40, 52);
    put16(&mut data, 46, 40); // e_shentsize
    put16(&mut data, 48, 5); // e_shnum
    put16(&mut data, 50, 1); // e_shstrndx

    let shstr = names();
    data[0x200..0x200 + shstr.len()].copy_from_slice(&shstr);
    data[0x240..0x248].copy_from_slice(&[0x90; 8]);
    let strtab = strings();
    data[0x300..0x300 + strtab.len()].copy_from_slice(&strtab);

    let base = 0x40 + 40;
    put32(&mut data, base, 1);
    put32(&mut data, base + 4, SHT_STRTAB);
    put32(&mut data, base + 16, 0x200);
    put32(&mut data, base + 20, shstr.len() as u32);
    put32(&mut data, base + 32, 1);

    let base = 0x40 + 2 * 40;
    put32(&mut data, base, 11);
    put32(&mut data, base + 4, 1);
    put32(&mut data, base + 8, 0x6);
    put32(&mut data, base + 12, 0x1000);
    put32(&mut data, base + 16, 0x240);
    put32(&mut data, base + 20, 8);
    put32(&mut data, base + 32, 16);

    let base = 0x40 + 3 * 40;
    put32(&mut data, base, 17);
    put32(&mut data, base + 4, SHT_SYMTAB);
    put32(&mut data, base + 16, 0x280);
    put32(&mut data, base + 20, 2 * 16);
    put32(&mut data, base + 24, 4);
    put32(&mut data, base + 28, 1);
    put32(&mut data, base + 32, 4);
    put32(&mut data, base + 36, 16);

    let base = 0x40 + 4 * 40;
    put32(&mut data, base, 25);
    put32(&mut data, base + 4, SHT_STRTAB);
    put32(&mut data, base + 16, 0x300);
    put32(&mut data, base + 20, strtab.len() as u32);
    put32(&mut data, base + 32, 1);

    let sym = 0x280 + 16;
    put32(&mut data, sym, 1);
    data[sym + 12] = (STB_GLOBAL << 4) | STT_FUNC;
    put16(&mut data, sym + 14, 2);
    put32(&mut data, sym + 4, 0x1000);
    put32(&mut data, sym + 8, 8);
    data
}

#[test]
fn elf64_sections_and_symbols_are_exposed() {
    let data = elf64();
    let object = Object::parse(&data).expect("valid ELF64");
    assert_eq!(object.class(), Class::Elf64);
    assert_eq!(object.object_type(), 1);
    assert_eq!(object.section_count(), 5);
    assert_eq!(
        object.section(1).and_then(|section| section.name()),
        Some(".shstrtab")
    );
    assert_eq!(
        object.section(2).and_then(|section| section.name()),
        Some(".text")
    );
    assert_eq!(
        object.section(2).map(|section| section.section_type()),
        Some(1)
    );
    assert_eq!(
        object.section(2).map(|section| section.offset()),
        Some(0x240)
    );
    assert_eq!(object.section(2).map(|section| section.size()), Some(8));
    assert_eq!(
        object.section(3).map(|section| section.section_type()),
        Some(SHT_SYMTAB)
    );
    assert_eq!(object.section(3).map(|section| section.entsize()), Some(24));
    assert_eq!(object.string(4, 1), Some("foo"));
    assert_eq!(object.string(4, 5), Some("bar"));
    assert_eq!(object.string(4, 100), None);

    let symbols = object.symbols().collect::<Vec<_>>();
    assert_eq!(symbols.len(), 3);
    assert_eq!(symbols[1].name(), Some("foo"));
    assert_eq!(symbols[1].value(), 0x1000);
    assert_eq!(symbols[1].size(), 8);
    assert_eq!(symbols[1].kind(), STT_FUNC);
    assert_eq!(symbols[1].binding(), STB_GLOBAL);
    assert_eq!(symbols[1].visibility(), 0);
    assert_eq!(symbols[1].section_index(), 2);
    assert_eq!(symbols[2].name(), Some("bar"));
    assert_eq!(symbols[2].kind(), STT_OBJECT);
    assert_eq!(symbols[2].binding(), STB_LOCAL);
    assert_eq!(symbols[2].section_index(), SHN_UNDEF);
}

#[test]
fn elf32_sections_and_symbols_are_exposed() {
    let data = elf32();
    let object = Object::parse(&data).expect("valid ELF32");
    assert_eq!(object.class(), Class::Elf32);
    assert_eq!(object.section_count(), 5);
    assert_eq!(
        object.section(2).and_then(|section| section.name()),
        Some(".text")
    );
    assert_eq!(object.section(3).map(|section| section.entsize()), Some(16));
    let symbols = object.symbols().collect::<Vec<_>>();
    assert_eq!(symbols.len(), 2);
    assert_eq!(symbols[1].name(), Some("foo"));
    assert_eq!(symbols[1].value(), 0x1000);
    assert_eq!(symbols[1].size(), 8);
    assert_eq!(symbols[1].kind(), STT_FUNC);
    assert_eq!(symbols[1].section_index(), 2);
}

#[test]
fn truncated_section_table_is_rejected() {
    let data = elf64();
    // The table runs from 0x40 to 0x180; any shorter slice must be rejected.
    let truncated = &data[..0x100];
    assert_eq!(Object::parse(truncated).unwrap_err(), ElfError::BadSection);
}

#[test]
fn a_section_name_outside_the_string_table_is_none() {
    let mut data = elf64();
    // Section 2's sh_name points past the shstrtab.
    put32(&mut data, 0x40 + 2 * 64, 0xffff);
    let object = Object::parse(&data).expect("valid table");
    assert_eq!(object.section(2).and_then(|section| section.name()), None);
}

#[test]
fn section_data_is_bounded() {
    let mut data = elf64();
    let section = object_section(&data, 2);
    assert_eq!(section.data(), &[0x90; 8]);
    // Make the section range run past EOF; data() must return empty, not panic.
    put64(&mut data, 0x40 + 2 * 64 + 24, 0x3ff);
    put64(&mut data, 0x40 + 2 * 64 + 32, 0x100);
    let section = object_section(&data, 2);
    assert!(section.data().is_empty());
}

fn object_section(data: &[u8], index: usize) -> Section<'_> {
    Object::parse(data)
        .expect("valid object")
        .section(index)
        .expect("section")
}
