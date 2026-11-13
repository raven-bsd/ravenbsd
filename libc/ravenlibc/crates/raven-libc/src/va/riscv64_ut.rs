// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests of the riscv64 psABI layout and reader (`riscv64.rs`).

use super::*;
use crate::va::VaReaderExt;

/// The save area followed by the caller's stack arguments: one contiguous array.
#[repr(C, align(16))]
struct Frame {
    save: VaSaveRiscv64,
    stack: [u64; 16],
}

impl Frame {
    fn new(named: usize) -> Self {
        debug_assert!(named <= A_SLOTS);
        Self {
            save: VaSaveRiscv64 {
                _ra_pad: [0; 2],
                a: [0; A_SLOTS],
            },
            stack: [0; 16],
        }
    }

    /// The cursor of a stub with `named` fixed arguments: the first free `a` slot.
    fn cursor(&self, named: usize) -> *mut u8 {
        let base = core::ptr::addr_of!(self.save).cast::<u8>();
        // SAFETY: `SAVE_OFFSET + 8*named` is inside the 80-byte save area for `named <= 8`.
        unsafe { base.add(SAVE_OFFSET + 8 * named).cast_mut() }
    }

    fn reader(&self, named: usize) -> RiscvReader {
        // SAFETY: the cursor points into the live frame.
        unsafe { RiscvReader::from_cursor(self.cursor(named)) }
    }
}

#[test]
fn layout_is_the_contract() {
    assert_eq!(core::mem::size_of::<VaSaveRiscv64>(), 80);
    assert_eq!(core::mem::align_of::<VaSaveRiscv64>(), 16);
    assert_eq!(core::mem::offset_of!(VaSaveRiscv64, a), 16);
    assert_eq!(FRAME_SIZE, 80);
    assert_eq!(SAVE_OFFSET + A_SLOTS * 8, SAVE_SIZE);
}

#[test]
fn eight_registers_then_the_contiguous_stack() {
    let mut frame = Frame::new(0);
    for (index, slot) in frame.save.a.iter_mut().enumerate() {
        *slot = 0x3300_0000_0000_0000 | index as u64;
    }
    frame.stack[0] = 0x9900;
    let mut reader = frame.reader(0);
    for index in 0..8 {
        assert_eq!(reader.next_u64(), 0x3300_0000_0000_0000 | index);
    }
    // The save area ends exactly at the stack: no gap, no padding.
    assert_eq!(reader.next_u64(), 0x9900);
}

#[test]
fn named_arguments_move_the_cursor() {
    let mut frame = Frame::new(3);
    frame.save.a[3] = 0x1234;
    let mut reader = frame.reader(3);
    assert_eq!(reader.next_u64(), 0x1234);
    let mut count = 0;
    while count < 4 {
        assert_eq!(reader.next_u64(), 0);
        count += 1;
    }
    assert_eq!(reader.cursor() as usize, frame.stack.as_ptr() as usize);
}

#[test]
fn doubles_use_the_integer_convention() {
    let mut frame = Frame::new(0);
    frame.save.a[0] = 0x4008_0000_0000_0000;
    frame.stack[0] = 0x4010_0000_0000_0000;
    let mut reader = frame.reader(0);
    assert_eq!(reader.next_f64_bits(), 0x4008_0000_0000_0000);
    for _ in 0..7 {
        let _ = reader.next_u64();
    }
    assert_eq!(reader.next_f64_bits(), 0x4010_0000_0000_0000);
}

#[test]
fn long_double_is_16_aligned_in_the_contiguous_area() {
    let value: u128 = 0x4000_aaaa_bbbb_cccc_dddd_eeee_ffff_0001;
    let frame = Frame::new(1);
    // a0 is the named word; the long double follows at a0+8, which is 8-aligned only.
    let base = frame.cursor(1) as usize;
    // SAFETY: the aligned slot and the following 16 bytes stay inside the frame.
    unsafe {
        let aligned = align_up_16(base) as *mut u8;
        core::ptr::write_unaligned(aligned.cast::<u128>(), value);
    }
    let mut reader = frame.reader(1);
    assert_eq!(reader.next_ld_bits(), value);
    assert_eq!(reader.cursor() as usize, align_up_16(base).wrapping_add(16));
}

#[test]
fn aggregates_read_from_the_contiguous_area() {
    let mut frame = Frame::new(7);
    frame.save.a[7] = 0x7777_8888_9999_aaaa;
    frame.stack[0] = 0xbbbb_cccc_dddd_eeee;
    let mut reader = frame.reader(7);
    let mut out = [0u8; 32];
    // 16 bytes cross the register/stack boundary: the memory is contiguous.
    assert!(reader.next_aggregate_int(16, 8, &mut out));
    assert_eq!(
        u64::from_le_bytes(out[0..8].try_into().unwrap()),
        0x7777_8888_9999_aaaa
    );
    assert_eq!(
        u64::from_le_bytes(out[8..16].try_into().unwrap()),
        0xbbbb_cccc_dddd_eeee
    );
    assert_eq!(reader.cursor() as usize, frame.stack.as_ptr() as usize + 8);
}

#[test]
fn a_large_aggregate_is_passed_by_reference() {
    let payload = [0x3Cu8; 24];
    let mut frame = Frame::new(0);
    frame.save.a[0] = payload.as_ptr() as u64;
    let mut reader = frame.reader(0);
    let mut out = [0u8; 32];
    assert!(reader.next_aggregate_int(24, 8, &mut out));
    assert_eq!(out[..24], payload);
    assert_eq!(reader.cursor() as usize, frame.cursor(1) as usize);
}

#[test]
fn aggregates_outside_the_subset_consume_nothing() {
    let mut frame = Frame::new(0);
    frame.save.a[0] = 0x5555;
    let mut reader = frame.reader(0);
    let mut out = [0u8; 32];
    assert!(!reader.next_aggregate_int(4, 4, &mut out));
    assert!(!reader.next_aggregate_int(8, 16, &mut out));
    assert_eq!(reader.next_u64(), 0x5555);
}

#[test]
fn clone_and_no_write_back() {
    let mut frame = Frame::new(0);
    for (index, slot) in frame.save.a.iter_mut().enumerate() {
        *slot = index as u64 + 1;
    }
    frame.stack[0] = 9;
    let before = frame.cursor(0);
    let mut first = frame.reader(0);
    assert_eq!(first.next_u64(), 1);
    let mut copy = first.clone_state();
    assert_eq!(copy.next_u64(), 2);
    assert_eq!(first.next_u64(), 2);
    for expected in 3..=9 {
        assert_eq!(copy.next_u64(), expected);
    }
    // The caller's va_list is a by-value cursor: the frame did not move.
    assert_eq!(frame.cursor(0), before);
}
