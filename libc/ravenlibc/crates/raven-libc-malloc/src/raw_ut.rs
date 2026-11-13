// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn integers_roundtrip_at_unaligned_addresses() {
    let mut buffer = [0u8; 32];
    let address = buffer.as_mut_ptr() as usize;
    write_u8(address + 1, 0xab);
    write_u16(address + 3, 0x1234);
    write_u32(address + 5, 0x1234_5678);
    write_u64(address + 9, 0x0123_4567_89ab_cdef);
    write_usize(address + 17, 0xdead_beef);
    assert_eq!(read_u8(address + 1), 0xab);
    assert_eq!(read_u16(address + 3), 0x1234);
    assert_eq!(read_u32(address + 5), 0x1234_5678);
    assert_eq!(read_u64(address + 9), 0x0123_4567_89ab_cdef);
    assert_eq!(read_usize(address + 17), 0xdead_beef);
}

#[test]
fn fill_copy_compare_and_overlap() {
    let mut buffer = [0u8; 32];
    let address = buffer.as_mut_ptr() as usize;
    fill(address, 0x5a, 8);
    assert_eq!(&buffer[..8], &[0x5a; 8]);
    write_bytes(address + 8, b"raven");
    assert_eq!(&buffer[8..13], b"raven");
    copy(address + 16, address + 8, 5);
    assert_eq!(&buffer[16..21], b"raven");
    copy_overlapping(address + 17, address + 16, 5);
    assert_eq!(&buffer[17..22], b"raven");
    assert_eq!(compare(address + 8, address + 17, 5), 0);
    fill_volatile(address, 0, 0);
    fill_volatile(address + 24, 0x7f, 8);
    assert_eq!(&buffer[24..], &[0x7f; 8]);
}

#[test]
fn read_bytes_round_trips() {
    let mut source = *b"slice";
    let mut target = [0u8; 5];
    let address = source.as_mut_ptr() as usize;
    read_bytes(address, &mut target);
    assert_eq!(&target, b"slice");
}

#[test]
fn sync_cell_is_shared_and_copyable() {
    let cell = SyncCell::new(7usize);
    cell.set(9);
    assert_eq!(cell.get(), 9);
}

#[test]
fn zero_length_operations_are_no_ops() {
    let mut buffer = [0u8; 8];
    let address = buffer.as_mut_ptr() as usize;
    fill(address, 0xaa, 0);
    fill_volatile(address, 0xbb, 0);
    copy(address, address, 0);
    copy_overlapping(address, address, 0);
    assert_eq!(buffer, [0; 8]);
}

#[test]
fn compare_reports_the_first_difference() {
    let left = [1u8, 2, 3, 4];
    let right = [1u8, 2, 9, 4];
    let value = compare(left.as_ptr() as usize, right.as_ptr() as usize, left.len());
    assert_eq!(value, -6);
}

#[test]
fn copy_zeroes_are_independent() {
    let mut buffer = [0xffu8; 16];
    let address = buffer.as_mut_ptr() as usize;
    copy(address + 8, address, 8);
    assert_eq!(&buffer[8..], &[0xff; 8]);
}

#[test]
fn write_and_read_bytes_are_symmetric() {
    let mut source = *b"abcdefgh";
    let mut target = [0u8; 8];
    let address = source.as_mut_ptr() as usize;
    read_bytes(address, &mut target);
    assert_eq!(&target, b"abcdefgh");
    let replacement = *b"12345678";
    write_bytes(address, &replacement);
    assert_eq!(&source, b"12345678");
}

#[test]
fn sync_cell_can_be_replaced_repeatedly() {
    let cell = SyncCell::new(0usize);
    let mut index = 0usize;
    while index < 100 {
        cell.set(index);
        assert_eq!(cell.get(), index);
        index += 1;
    }
}

#[test]
fn u16_round_trip_at_the_last_two_bytes() {
    let mut buffer = [0u8; 2];
    let address = buffer.as_mut_ptr() as usize;
    write_u16(address, 0xabcd);
    assert_eq!(read_u16(address), 0xabcd);
}
