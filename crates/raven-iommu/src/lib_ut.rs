// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Host tests for the VT-d table, allocator and fault helpers.

use super::*;

fn put_u16(bytes: &mut [u8], offset: usize, value: u16) {
    bytes[offset..offset + 2].copy_from_slice(&value.to_le_bytes());
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

fn put_u64(bytes: &mut [u8], offset: usize, value: u64) {
    bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

fn finish_checksum(bytes: &mut [u8]) {
    bytes[9] = 0;
    let sum = bytes.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte));
    bytes[9] = sum.wrapping_neg();
}

fn sample_dmar() -> Vec<u8> {
    let mut bytes = vec![0u8; 96];
    bytes[..4].copy_from_slice(b"DMAR");
    put_u32(&mut bytes, 4, 96);
    bytes[8] = 1;
    bytes[10..16].copy_from_slice(b"RAVEN ");
    bytes[16..24].copy_from_slice(b"RavenBSD");
    put_u32(&mut bytes, 24, 1);
    bytes[28..32].copy_from_slice(b"TEST");
    bytes[36] = 39;
    bytes[37] = 0;
    put_u16(&mut bytes, 48, DMAR_DRHD);
    put_u16(&mut bytes, 50, 16);
    bytes[52] = 1;
    put_u16(&mut bytes, 54, 0);
    put_u64(&mut bytes, 56, 0xFED9_0000);
    put_u16(&mut bytes, 64, DMAR_RMRR);
    put_u16(&mut bytes, 66, 24);
    put_u16(&mut bytes, 68, 0);
    put_u64(&mut bytes, 72, 0x8000_0000);
    put_u64(&mut bytes, 80, 0x8000_0FFF);
    put_u16(&mut bytes, 88, DMAR_ATSR);
    put_u16(&mut bytes, 90, 8);
    bytes[92] = 1;
    put_u16(&mut bytes, 94, 0);
    finish_checksum(&mut bytes);
    bytes
}

#[test]
fn parses_drhd_rmrr_and_atsr() {
    let table = DmarTable::parse(&sample_dmar()).expect("parse");
    assert_eq!(table.host_address_width, 39);
    assert_eq!(table.units.len(), 1);
    assert!(table.first_unit().include_all);
    assert_eq!(table.first_unit().base, 0xFED9_0000);
    assert_eq!(table.rmrr_count(), 1);
    assert_eq!(table.rmrr[0].base, 0x8000_0000);
    assert_eq!(table.rmrr[0].limit, 0x8000_0FFF);
    assert_eq!(table.atsr.len(), 1);
    assert!(table.atsr[0].all_ports);
}

#[test]
fn rejects_bad_signature_checksum_and_length() {
    let mut bytes = sample_dmar();
    bytes[0] = b'X';
    assert_eq!(DmarTable::parse(&bytes), Err(DmarError::BadSignature));

    let mut bytes = sample_dmar();
    bytes[9] = bytes[9].wrapping_add(1);
    assert_eq!(DmarTable::parse(&bytes), Err(DmarError::BadChecksum));

    let mut bytes = sample_dmar();
    put_u32(&mut bytes, 4, 48);
    bytes.truncate(48);
    finish_checksum(&mut bytes);
    assert_eq!(DmarTable::parse(&bytes), Err(DmarError::NoUnits));

    let mut bytes = sample_dmar();
    put_u16(&mut bytes, 50, 0);
    finish_checksum(&mut bytes);
    assert_eq!(DmarTable::parse(&bytes), Err(DmarError::BadLength));
}

#[test]
fn rejects_bad_drhd_base() {
    let mut bytes = sample_dmar();
    put_u64(&mut bytes, 56, 0x1234);
    finish_checksum(&mut bytes);
    assert_eq!(DmarTable::parse(&bytes), Err(DmarError::BadUnit));
}

#[test]
fn root_and_context_entries_keep_the_domain_and_table() {
    let root = root_entry(0x1234_5000);
    assert_eq!(root & ROOT_PRESENT, ROOT_PRESENT);
    assert_eq!(root & CONTEXT_ASR_MASK, 0x1234_5000);

    let (low, high) = context_entry(0x1234, 3, 0x5678_9000);
    assert_eq!(low & CONTEXT_PRESENT, CONTEXT_PRESENT);
    assert_eq!(low & CONTEXT_ASR_MASK, 0x5678_9000);
    assert_eq!((high >> 8) & 0xFFFF, 0x1234);
    assert_eq!(high & 0x3, 1);
}

#[test]
fn agaw_and_table_levels_agree() {
    assert_eq!(levels_for_agaw(30), Some(2));
    assert_eq!(levels_for_agaw(39), Some(3));
    assert_eq!(levels_for_agaw(48), Some(4));
    assert_eq!(levels_for_agaw(57), None);
    assert_eq!(agaw_code(2), 0);
    assert_eq!(agaw_code(3), 1);
    assert_eq!(agaw_code(4), 2);
}

#[test]
fn table_indexes_use_next_page_bits() {
    assert_eq!(table_index(3, 0, 0x0000_0001_0000_0000), 4);
    assert_eq!(table_index(3, 1, 0x0000_0000_0020_0000), 1);
    assert_eq!(table_index(3, 2, 0x0000_0000_0000_3000), 3);
    assert_eq!(table_index(2, 0, 0x0000_0000_0040_0000), 2);
    assert_eq!(table_index(4, 3, 0x0000_0080_0000_0000), 0);
}

#[test]
fn page_entry_sets_access_bits() {
    assert_eq!(page_entry(0x1234_5000, false), 0x1234_5001);
    assert_eq!(page_entry(0x1234_5000, true), 0x1234_5003);
}

#[test]
fn iova_allocator_is_first_fit_and_coalesces() {
    let mut allocator = IovaAllocator::new(0x10_0000, 0x10_5000).expect("allocator");
    assert_eq!(allocator.free_bytes(), 0x5000);
    assert_eq!(allocator.allocate(0x1000, 0x1000), Some(0x10_0000));
    assert_eq!(allocator.allocate(0x1000, 0x1000), Some(0x10_1000));
    assert_eq!(allocator.free_bytes(), 0x3000);
    allocator.release(0x10_0000, 0x1000).expect("release");
    allocator.release(0x10_1000, 0x1000).expect("release");
    assert_eq!(allocator.free_bytes(), 0x5000);
    assert_eq!(allocator.allocate(0x2000, 0x1000), Some(0x10_0000));
    assert_eq!(allocator.allocate(0x2000, 0x1000), Some(0x10_2000));
    assert_eq!(allocator.allocate(0x1000, 0x1000), Some(0x10_4000));
    assert_eq!(allocator.allocate(0x1000, 0x1000), None);
    allocator.release(0x10_0000, 0x5000).expect("release all");
    assert_eq!(allocator.free_bytes(), 0x5000);
}

#[test]
fn iova_allocator_honours_alignment_and_bounds() {
    let mut allocator = IovaAllocator::new(0x10_0000, 0x10_4000).expect("allocator");
    assert_eq!(allocator.allocate(0x1000, 0x2000), Some(0x10_0000));
    assert_eq!(allocator.allocate(0x1000, 0x1000), Some(0x10_1000));
    assert_eq!(allocator.allocate(0x2000, 0x1000), Some(0x10_2000));
    assert_eq!(allocator.allocate(0x1000, 0x1000), None);
    assert_eq!(
        allocator.release(0x10_5000, 0x1000),
        Err(IovaError::NotAllocated)
    );
    assert_eq!(
        allocator.release(0x10_0000, 0x800),
        Err(IovaError::Misaligned)
    );
    let mut limited = IovaAllocator::new(0x20_0000, 0x30_0000).expect("limited");
    assert_eq!(
        limited.allocate_within(0x1000, 0x1000, 0x20_2000),
        Some(0x20_0000)
    );
    assert_eq!(limited.allocate_within(0x2000, 0x1000, 0x20_2000), None);
    assert_eq!(
        limited.allocate_within(0x1000, 0x1000, 0x20_3000),
        Some(0x20_1000)
    );
}

#[test]
fn fault_record_decodes_reason_sid_and_address() {
    let high = (1u64 << 63) | (1u64 << 62) | (0x04u64 << 32) | 0x18;
    let fault = decode_fault(0x1234_5000, high);
    assert!(fault.present);
    assert!(fault.write);
    assert_eq!(fault.reason, 0x04);
    assert_eq!(fault.source_id, 0x18);
    assert_eq!(fault.address, 0x1234_5000);
    assert_eq!(fault.address_type, 0);
    assert_eq!(
        fault_reason_text(fault.reason),
        "second-level entry not present"
    );
}
