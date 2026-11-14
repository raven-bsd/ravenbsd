// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

use super::*;

#[test]
fn empty_input_reads_zero() {
    let mut cursor = Cursor::new(&[]);
    assert_eq!(cursor.u8(), 0);
    assert_eq!(cursor.u16(), 0);
    assert_eq!(cursor.u32(), 0);
    assert_eq!(cursor.u64(), 0);
    assert_eq!(cursor.remaining(), 0);
}

#[test]
fn a_short_tail_zero_fills_the_word() {
    let mut cursor = Cursor::new(&[0x01, 0x02, 0x03]);
    assert_eq!(cursor.u32(), 0x0003_0201);
}

#[test]
fn take_shortens_at_the_end() {
    let mut cursor = Cursor::new(b"abcdef");
    assert_eq!(cursor.take(2), b"ab");
    assert_eq!(cursor.take(10), b"cdef");
    assert_eq!(cursor.take(1), b"");
}

#[test]
fn rest_does_not_advance() {
    let mut cursor = Cursor::new(b"abcdef");
    assert_eq!(cursor.take(2), b"ab");
    assert_eq!(cursor.rest(), b"cdef");
    assert_eq!(cursor.rest(), b"cdef");
    assert_eq!(cursor.take(9), b"cdef");
    assert_eq!(cursor.rest(), b"");
}

#[test]
fn bool_uses_the_low_bit() {
    let mut cursor = Cursor::new(&[0, 1, 2, 3, 0xff]);
    assert!(!cursor.bool());
    assert!(cursor.bool());
    assert!(!cursor.bool());
    assert!(cursor.bool());
    assert!(cursor.bool());
}

#[test]
fn pick_wraps_by_the_next_byte() {
    let mut cursor = Cursor::new(&[0, 1, 2, 4]);
    let items = [10u8, 20, 30];
    assert_eq!(cursor.pick(&items), 10);
    assert_eq!(cursor.pick(&items), 20);
    assert_eq!(cursor.pick(&items), 30);
    assert_eq!(cursor.pick(&items), 20);
}

#[test]
fn remaining_counts_down() {
    let mut cursor = Cursor::new(&[1, 2, 3]);
    assert_eq!(cursor.remaining(), 3);
    let _ = cursor.u8();
    assert_eq!(cursor.remaining(), 2);
    let _ = cursor.u64();
    assert_eq!(cursor.remaining(), 0);
}

#[test]
fn a_zero_length_take_does_not_advance() {
    let mut cursor = Cursor::new(b"abc");
    assert_eq!(cursor.take(0), b"");
    assert_eq!(cursor.remaining(), 3);
    assert_eq!(cursor.rest(), b"abc");
}

#[test]
fn u16_is_little_endian() {
    let mut cursor = Cursor::new(&[0x34, 0x12]);
    assert_eq!(cursor.u16(), 0x1234);
}

#[test]
fn pick_requires_a_non_empty_table() {
    let mut cursor = Cursor::new(&[9]);
    assert_eq!(cursor.pick(&[42u8]), 42);
}

#[test]
fn positions_do_not_overflow() {
    let mut cursor = Cursor::new(&[0xaa]);
    for _ in 0..32 {
        let _ = cursor.u64();
    }
    assert_eq!(cursor.remaining(), 0);
}
