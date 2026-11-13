// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]
// Record construction is over fixed little-endian literals; an overflow would fail the test itself.

//! ELF record sizes, offsets, truncation and validation.

use super::*;
use crate::mem::SliceMem;

fn put16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn ehdr_bytes() -> Vec<u8> {
    let mut bytes = vec![0u8; EHDR_SIZE];
    bytes[0..4].copy_from_slice(&ELF_MAGIC);
    bytes[4] = ELFCLASS64;
    bytes[5] = ELFDATA2LSB;
    bytes[6] = EV_CURRENT;
    put16(&mut bytes, 16, ET_DYN);
    put16(&mut bytes, 18, EM_X86_64);
    put32(&mut bytes, 20, u32::from(EV_CURRENT));
    put64(&mut bytes, 24, 0x1234);
    put64(&mut bytes, 32, 0x40);
    put16(&mut bytes, 54, PHDR_SIZE as u16);
    put16(&mut bytes, 56, 3);
    bytes
}

#[test]
fn the_record_sizes_are_the_abi_sizes() {
    let mut phdr = vec![0u8; PHDR_SIZE];
    put32(&mut phdr, 0, 1);
    assert!(Phdr::parse(&phdr).is_ok());
    assert!(Phdr::parse(&phdr[..PHDR_SIZE - 1]).is_err());
    let mut dyn_entry = vec![0u8; 16];
    put64(&mut dyn_entry, 0, 1);
    assert!(Dyn::parse(&dyn_entry).is_ok());
    assert!(Dyn::parse(&dyn_entry[..15]).is_err());
    let sym = vec![0u8; SYM_SIZE];
    assert!(Sym::parse(&sym).is_ok());
    assert!(Sym::parse(&sym[..SYM_SIZE - 1]).is_err());
    let rela = vec![0u8; RELA_SIZE];
    assert!(Rela::parse(&rela).is_ok());
    assert!(Rela::parse(&rela[..RELA_SIZE - 1]).is_err());
    let verdef = vec![0u8; 20];
    assert!(Verdef::parse(&verdef).is_ok());
    assert!(Verdef::parse(&verdef[..19]).is_err());
    let verdaux = vec![0u8; 8];
    assert!(Verdaux::parse(&verdaux).is_ok());
    assert!(Verdaux::parse(&verdaux[..7]).is_err());
    let verneed = vec![0u8; 16];
    assert!(Verneed::parse(&verneed).is_ok());
    assert!(Verneed::parse(&verneed[..15]).is_err());
    let vernaux = vec![0u8; 16];
    assert!(Vernaux::parse(&vernaux).is_ok());
    assert!(Vernaux::parse(&vernaux[..15]).is_err());
}

#[test]
fn the_header_round_trips() {
    let bytes = ehdr_bytes();
    let header = Ehdr::parse(&bytes).unwrap();
    assert_eq!(header.e_type, ET_DYN);
    assert_eq!(header.e_machine, EM_X86_64);
    assert_eq!(header.e_entry, 0x1234);
    assert_eq!(header.e_phoff, 0x40);
    assert_eq!(header.e_phentsize, 56);
    assert_eq!(header.e_phnum, 3);
}

#[test]
fn the_header_rejects_every_truncated_prefix() {
    let bytes = ehdr_bytes();
    for length in 0..EHDR_SIZE {
        let result = Ehdr::parse(&bytes[..length]);
        assert!(result.is_err(), "length {length} was accepted");
    }
    assert!(Ehdr::parse(&bytes).is_ok());
}

#[test]
fn the_header_rejects_a_bad_magic() {
    let mut bytes = ehdr_bytes();
    bytes[1] = b'X';
    assert_eq!(Ehdr::parse(&bytes), Err(ElfError::BadMagic));
}

#[test]
fn the_header_rejects_a_bad_class() {
    let mut bytes = ehdr_bytes();
    bytes[4] = 1;
    assert_eq!(Ehdr::parse(&bytes), Err(ElfError::BadClass));
}

#[test]
fn the_header_rejects_a_bad_endianness() {
    let mut bytes = ehdr_bytes();
    bytes[5] = 2;
    assert_eq!(Ehdr::parse(&bytes), Err(ElfError::BadEndian));
}

#[test]
fn the_header_rejects_a_bad_ident_version() {
    let mut bytes = ehdr_bytes();
    bytes[6] = 2;
    assert_eq!(Ehdr::parse(&bytes), Err(ElfError::BadVersion));
}

#[test]
fn the_header_rejects_a_bad_e_version() {
    let mut bytes = ehdr_bytes();
    put32(&mut bytes, 20, 2);
    assert_eq!(Ehdr::parse(&bytes), Err(ElfError::BadVersion));
}

#[test]
fn the_header_rejects_a_bad_phentsize() {
    let mut bytes = ehdr_bytes();
    put16(&mut bytes, 54, 32);
    assert_eq!(Ehdr::parse(&bytes), Err(ElfError::BadPhdrSize));
}

#[test]
fn the_header_rejects_too_many_program_headers() {
    let mut bytes = ehdr_bytes();
    put16(&mut bytes, 56, 65);
    assert_eq!(Ehdr::parse(&bytes), Err(ElfError::TooManyPhdrs));
    put16(&mut bytes, 56, 64);
    assert!(Ehdr::parse(&bytes).is_ok());
}

#[test]
fn the_header_rejects_relocatable_and_unknown_objects() {
    let mut bytes = ehdr_bytes();
    put16(&mut bytes, 16, 1);
    assert_eq!(Ehdr::parse(&bytes), Err(ElfError::UnsupportedType));
    put16(&mut bytes, 16, ET_EXEC);
    assert!(Ehdr::parse(&bytes).is_ok());
    put16(&mut bytes, 16, 0xff00);
    assert_eq!(Ehdr::parse(&bytes), Err(ElfError::UnsupportedType));
}

#[test]
fn a_program_header_reads_every_field_from_its_offset() {
    let mut bytes = vec![0u8; PHDR_SIZE];
    put32(&mut bytes, 0, 1);
    put32(&mut bytes, 4, 5);
    put64(&mut bytes, 8, 0x100);
    put64(&mut bytes, 16, 0x200);
    put64(&mut bytes, 24, 0x300);
    put64(&mut bytes, 32, 0x400);
    put64(&mut bytes, 40, 0x500);
    put64(&mut bytes, 48, 0x1000);
    let header = Phdr::parse(&bytes).unwrap();
    assert_eq!(header.p_type, 1);
    assert_eq!(header.p_flags, 5);
    assert_eq!(header.p_offset, 0x100);
    assert_eq!(header.p_vaddr, 0x200);
    assert_eq!(header.p_paddr, 0x300);
    assert_eq!(header.p_filesz, 0x400);
    assert_eq!(header.p_memsz, 0x500);
    assert_eq!(header.p_align, 0x1000);
}

#[test]
fn a_dynamic_entry_reads_both_words() {
    let mut bytes = vec![0u8; 16];
    put64(&mut bytes, 0, 30);
    put64(&mut bytes, 8, 0x8);
    let entry = Dyn::parse(&bytes).unwrap();
    assert_eq!(entry.tag, 30);
    assert_eq!(entry.val, 8);
}

#[test]
fn a_symbol_exposes_bind_type_and_visibility() {
    let mut bytes = vec![0u8; SYM_SIZE];
    put32(&mut bytes, 0, 0x42);
    bytes[4] = (2 << 4) | 10;
    bytes[5] = 3;
    put16(&mut bytes, 6, 0xfff1);
    put64(&mut bytes, 8, 0xdead);
    put64(&mut bytes, 16, 12);
    let symbol = Sym::parse(&bytes).unwrap();
    assert_eq!(symbol.name, 0x42);
    assert_eq!(symbol.bind(), 2);
    assert_eq!(symbol.typ(), 10);
    assert_eq!(symbol.visibility(), 3);
    assert_eq!(symbol.shndx, 0xfff1);
    assert_eq!(symbol.value, 0xdead);
    assert_eq!(symbol.size, 12);
}

#[test]
fn a_relocation_splits_info_into_symbol_and_type() {
    let mut bytes = vec![0u8; RELA_SIZE];
    put64(&mut bytes, 0, 0x1000);
    let info = (7u64 << 32) | 37;
    put64(&mut bytes, 8, info);
    put64(&mut bytes, 16, (-8i64) as u64);
    let rela = Rela::parse(&bytes).unwrap();
    assert_eq!(rela.offset, 0x1000);
    assert_eq!(rela.sym(), 7);
    assert_eq!(rela.typ(), 37);
    assert_eq!(rela.addend, -8);
}

#[test]
fn the_version_records_read_their_offsets() {
    let mut verdef = vec![0u8; 28];
    put16(&mut verdef, 0, 1);
    put16(&mut verdef, 2, 2);
    put16(&mut verdef, 4, 3);
    put16(&mut verdef, 6, 4);
    put32(&mut verdef, 8, 0xaabb);
    put32(&mut verdef, 12, 20);
    put32(&mut verdef, 16, 28);
    let def = Verdef::parse(&verdef).unwrap();
    assert_eq!(def.version, 1);
    assert_eq!(def.flags, 2);
    assert_eq!(def.ndx, 3);
    assert_eq!(def.cnt, 4);
    assert_eq!(def.hash, 0xaabb);
    assert_eq!(def.aux, 20);
    assert_eq!(def.next, 28);

    let mut verdaux = vec![0u8; 8];
    put32(&mut verdaux, 0, 9);
    put32(&mut verdaux, 4, 8);
    let aux = Verdaux::parse(&verdaux).unwrap();
    assert_eq!(aux.name, 9);
    assert_eq!(aux.next, 8);

    let mut verneed = vec![0u8; 16];
    put16(&mut verneed, 0, 1);
    put16(&mut verneed, 2, 2);
    put32(&mut verneed, 4, 3);
    put32(&mut verneed, 8, 16);
    put32(&mut verneed, 12, 32);
    let need = Verneed::parse(&verneed).unwrap();
    assert_eq!(need.version, 1);
    assert_eq!(need.cnt, 2);
    assert_eq!(need.file, 3);
    assert_eq!(need.aux, 16);
    assert_eq!(need.next, 32);

    let mut vernaux = vec![0u8; 16];
    put32(&mut vernaux, 0, 0x1234);
    put16(&mut vernaux, 4, 2);
    put16(&mut vernaux, 6, 3);
    put32(&mut vernaux, 8, 4);
    put32(&mut vernaux, 12, 16);
    let aux = Vernaux::parse(&vernaux).unwrap();
    assert_eq!(aux.hash, 0x1234);
    assert_eq!(aux.flags, 2);
    assert_eq!(aux.other, 3);
    assert_eq!(aux.name, 4);
    assert_eq!(aux.next, 16);
}

#[test]
fn a_note_reads_its_padded_records() {
    // namesz 4 ("GNU\0"), descsz 4, type 3, then name, padding, desc, padding.
    let mut bytes = vec![0u8; 12];
    put32(&mut bytes, 0, 4);
    put32(&mut bytes, 4, 4);
    put32(&mut bytes, 8, 3);
    bytes.extend_from_slice(b"GNU\0");
    bytes.extend_from_slice(b"desc");
    bytes.extend_from_slice(&[0u8; 4]);
    let note = Note::parse(&bytes).unwrap();
    assert_eq!(note.n_type, 3);
    assert_eq!(note.name, b"GNU\0");
    assert_eq!(note.desc, b"desc");
    assert_eq!(note.len, 20);
}

#[test]
fn a_note_with_a_short_header_is_truncated() {
    for length in 0..NOTE_HEADER_SIZE {
        assert_eq!(Note::parse(&vec![0u8; length]), Err(ElfError::Truncated));
    }
}

#[test]
fn a_note_overrunning_its_buffer_is_truncated() {
    let mut bytes = vec![0u8; 12];
    put32(&mut bytes, 0, 8);
    put32(&mut bytes, 4, 0);
    assert_eq!(Note::parse(&bytes), Err(ElfError::Truncated));
}

#[test]
fn program_headers_are_read_through_a_view() {
    let mut bytes = Vec::new();
    for index in 0..2u32 {
        let mut record = vec![0u8; PHDR_SIZE];
        put32(&mut record, 0, index + 1);
        put64(&mut record, 16, 0x1000);
        bytes.extend_from_slice(&record);
    }
    let view = SliceMem::new(0x4000, &bytes);
    let headers = read_phdrs(&view, 0x4000, 2).unwrap();
    assert_eq!(headers.len(), 2);
    assert_eq!(headers[0].p_type, 1);
    assert_eq!(headers[1].p_type, 2);
    assert_eq!(headers[0].p_vaddr, 0x1000);
}

#[test]
fn reading_program_headers_outside_the_view_fails() {
    let bytes = vec![0u8; PHDR_SIZE];
    let view = SliceMem::new(0x4000, &bytes);
    assert_eq!(read_phdrs(&view, 0x8000, 1), Err(ElfError::BadEntry));
    assert_eq!(read_phdrs(&view, u64::MAX, 1), Err(ElfError::BadEntry));
    assert_eq!(read_phdrs(&view, 0x4000, 65), Err(ElfError::TooManyPhdrs));
}

#[test]
fn an_unknown_machine_is_not_an_arch() {
    assert_eq!(Arch::from_machine(62), Some(Arch::X86_64));
    assert_eq!(Arch::from_machine(183), Some(Arch::Aarch64));
    assert_eq!(Arch::from_machine(243), Some(Arch::Riscv64));
    assert_eq!(Arch::from_machine(3), None);
    assert_eq!(Arch::X86_64.name(), "x86_64");
    assert_eq!(Arch::Aarch64.name(), "aarch64");
    assert_eq!(Arch::Riscv64.name(), "riscv64");
}
