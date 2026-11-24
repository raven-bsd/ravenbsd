// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the header-only `Elf::parse_with_len` path used by `PROC_EXEC`.

use super::*;
use std::vec;
use std::vec::Vec;

const USER_MAX: u64 = 0x7fff_ffff_ffff;
const BASE: u64 = 0x0040_0000;
const FILE_LEN: usize = 0x3000;

fn put_u16(data: &mut [u8], offset: usize, value: u16) {
    data[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(data: &mut [u8], offset: usize, value: u32) {
    data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

/// One static ET_EXEC with a single RX PT_LOAD at file offset 0x1000.
fn image() -> Vec<u8> {
    let mut data = vec![0u8; FILE_LEN];
    data[0..4].copy_from_slice(b"\x7fELF");
    data[4] = 2;
    data[5] = 1;
    put_u16(&mut data, 16, ET_EXEC);
    put_u16(&mut data, 18, EM_X86_64);
    put_u32(&mut data, 20, 1);
    put_u64(&mut data, 24, BASE);
    put_u64(&mut data, 32, 64);
    put_u16(&mut data, 52, 64);
    put_u16(&mut data, 54, PHDR_SIZE);
    put_u16(&mut data, 56, 1);

    let header = 64;
    put_u32(&mut data, header, PT_LOAD);
    put_u32(&mut data, header + 4, PF_R | PF_X);
    put_u64(&mut data, header + 8, 0x1000);
    put_u64(&mut data, header + 16, BASE);
    put_u64(&mut data, header + 24, BASE);
    put_u64(&mut data, header + 32, 0x2000);
    put_u64(&mut data, header + 40, 0x2000);
    put_u64(&mut data, header + 48, PAGE_SIZE);
    data[0x1000..0x1004].copy_from_slice(&[0xc3, 0x90, 0x90, 0x90]);
    data
}

#[test]
fn a_header_only_buffer_validates_against_the_real_image_length() {
    let data = image();
    let header = data.get(..64 + PHDR_SIZE as usize).unwrap();
    let elf = Elf::parse_with_len(header, FILE_LEN as u64, Machine::X86_64, 0..USER_MAX)
        .expect("header buffer must validate");
    assert_eq!(elf.entry(), BASE);
    assert_eq!(elf.phnum(), 1);
    let segment = elf.segments().next().unwrap();
    assert_eq!(segment.offset, 0x1000);
    assert_eq!(segment.filesz, 0x2000);
    // The header buffer deliberately does not contain the segment bytes.
    assert!(elf.segment_data(&segment).is_empty());
}

#[test]
fn a_short_declared_file_length_is_rejected() {
    let data = image();
    let header = data.get(..64 + PHDR_SIZE as usize).unwrap();
    let error = Elf::parse_with_len(header, 0x1800, Machine::X86_64, 0..USER_MAX).unwrap_err();
    assert_eq!(error, ElfError::SegmentOutOfBounds);
}

#[test]
fn a_header_only_buffer_is_not_a_full_image() {
    let data = image();
    let header = data.get(..64 + PHDR_SIZE as usize).unwrap();
    let error = Elf::parse(header, Machine::X86_64, 0..USER_MAX).unwrap_err();
    assert_eq!(error, ElfError::SegmentOutOfBounds);
}

#[test]
fn a_missing_program_header_table_is_rejected() {
    let data = image();
    let header = data.get(..64).unwrap();
    let error =
        Elf::parse_with_len(header, FILE_LEN as u64, Machine::X86_64, 0..USER_MAX).unwrap_err();
    assert_eq!(error, ElfError::BadPhdr);
}

#[test]
fn an_interp_beyond_the_file_length_is_rejected() {
    let mut data = image();
    put_u16(&mut data, 56, 2);
    let header = 64 + PHDR_SIZE as usize;
    put_u32(&mut data, header, PT_INTERP);
    put_u64(&mut data, header + 8, 0x4000);
    put_u64(&mut data, header + 32, 16);
    let table = data.get(..64 + 2 * PHDR_SIZE as usize).unwrap();
    let error =
        Elf::parse_with_len(table, FILE_LEN as u64, Machine::X86_64, 0..USER_MAX).unwrap_err();
    assert_eq!(error, ElfError::BadPhdr);
}
