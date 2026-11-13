// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests of the aarch64 AAPCS64 layout and reader (`aapcs64.rs`).

use super::*;
use crate::va::VaReaderExt;

/// A frame shaped like the stub's: save area, the list, then the caller's stack arguments.
#[repr(C, align(16))]
struct Frame {
    save: VaSaveAapcs64,
    list: VaListAapcs64,
    stack: [u64; 16],
}

impl Frame {
    fn new(named: usize) -> Self {
        Self {
            save: VaSaveAapcs64 {
                _fplr: [0; 2],
                gr: [0; 8],
                vr: [[0; 16]; 8],
            },
            list: VaListAapcs64 {
                stack: core::ptr::null_mut(),
                gr_top: core::ptr::null_mut(),
                vr_top: core::ptr::null_mut(),
                gr_offs: -((64 - 8 * named as i32) as i32),
                vr_offs: VR_OFFSET_INIT,
            },
            stack: [0; 16],
        }
    }

    fn wire(&mut self) {
        let base = core::ptr::addr_of_mut!(self.save).cast::<u8>();
        self.list.stack = self.stack.as_mut_ptr().cast::<u8>();
        // SAFETY: the offsets are the two field ends of the 208-byte save area.
        unsafe {
            self.list.gr_top = base.add(GR_TOP);
            self.list.vr_top = base.add(VR_TOP);
        }
    }

    fn reader(&self) -> Aapcs64Reader {
        Aapcs64Reader::from_list(self.list)
    }
}

#[test]
fn layout_is_the_contract() {
    assert_eq!(core::mem::size_of::<VaListAapcs64>(), 32);
    assert_eq!(core::mem::align_of::<VaListAapcs64>(), 8);
    assert_eq!(core::mem::offset_of!(VaListAapcs64, stack), 0);
    assert_eq!(core::mem::offset_of!(VaListAapcs64, gr_top), 8);
    assert_eq!(core::mem::offset_of!(VaListAapcs64, vr_top), 16);
    assert_eq!(core::mem::offset_of!(VaListAapcs64, gr_offs), 24);
    assert_eq!(core::mem::offset_of!(VaListAapcs64, vr_offs), 28);
    assert_eq!(core::mem::size_of::<VaSaveAapcs64>(), 208);
    assert_eq!(core::mem::align_of::<VaSaveAapcs64>(), 16);
    assert_eq!(FRAME_SIZE, 240);
    assert_eq!(LIST_OFFSET, 208);
}

#[test]
fn eight_integers_use_the_gr_area_and_the_ninth_the_stack() {
    let mut frame = Frame::new(0);
    for (index, slot) in frame.save.gr.iter_mut().enumerate() {
        *slot = 0x2200_0000_0000_0000 | index as u64;
    }
    frame.stack[0] = 0x9000;
    frame.wire();
    let mut reader = frame.reader();
    for index in 0..8 {
        assert_eq!(reader.next_u64(), 0x2200_0000_0000_0000 | index);
    }
    assert_eq!(reader.next_u64(), 0x9000);
    assert!(reader.list().gr_offs >= 0);
}

#[test]
fn named_arguments_consume_gr_slots() {
    let mut frame = Frame::new(3);
    frame.save.gr[3] = 0x3333;
    frame.stack[0] = 0x8888;
    frame.wire();
    let mut reader = frame.reader();
    assert_eq!(reader.list().gr_offs, -40);
    assert_eq!(reader.next_u64(), 0x3333);
    for _ in 0..4 {
        assert_eq!(reader.next_u64(), 0);
    }
    // The eighth slot is used, the next one overflows to the stack.
    assert_eq!(reader.next_u64(), 0x8888);
}

#[test]
fn eight_doubles_use_the_vr_area_and_the_ninth_the_stack() {
    let mut frame = Frame::new(0);
    for (index, slot) in frame.save.vr.iter_mut().enumerate() {
        slot[..8].copy_from_slice(&(0x3ff0_0000_0000_0000u64 | index as u64).to_le_bytes());
        for byte in &mut slot[8..] {
            *byte = 0xA5;
        }
    }
    frame.stack[0] = 0x4242;
    frame.wire();
    let mut reader = frame.reader();
    for index in 0..8 {
        assert_eq!(reader.next_f64_bits(), 0x3ff0_0000_0000_0000 | index);
    }
    assert_eq!(reader.next_f64_bits(), 0x4242);
}

#[test]
fn soft_float_vr_offs_zero_sends_doubles_to_the_stack() {
    let mut frame = Frame::new(0);
    frame.list.vr_offs = VR_OFFSET_SOFTFLOAT;
    frame.save.vr[0][..8].copy_from_slice(&0x1234u64.to_le_bytes());
    frame.stack[0] = 0x5678;
    frame.wire();
    let mut reader = frame.reader();
    assert_eq!(reader.next_f64_bits(), 0x5678);
    assert_eq!(reader.list().vr_offs, 0);
}

#[test]
fn long_double_uses_a_vr_slot() {
    let mut frame = Frame::new(0);
    let value: u128 = 0x4000_0123_4567_89ab_cdef_0011_2233_4455;
    frame.save.vr[0].copy_from_slice(&value.to_le_bytes());
    frame.wire();
    let mut reader = frame.reader();
    assert_eq!(reader.next_ld_bits(), value);
    assert_eq!(reader.list().vr_offs, -112);
}

#[test]
fn long_double_on_the_stack_is_16_aligned() {
    let mut frame = Frame::new(0);
    let value: u128 = 0x4000_0123_4567_89ab_cdef_0011_2233_4455;
    frame.wire();
    // No vector register left, and the stack pointer is deliberately 8-aligned only.
    frame.list.vr_offs = VR_OFFSET_SOFTFLOAT;
    // SAFETY: `raw` points at the frame's own 128-byte array.
    let misaligned = unsafe { frame.stack.as_mut_ptr().cast::<u8>().add(8) };
    frame.list.stack = misaligned;
    // SAFETY: the aligned slot is inside the frame's own stack array.
    unsafe {
        core::ptr::write_unaligned(align_up_16(misaligned as usize) as *mut u128, value);
    }
    let mut reader = Aapcs64Reader::from_list(frame.list);
    assert_eq!(reader.next_ld_bits(), value);
    assert_eq!(
        reader.list().stack as usize,
        align_up_16(misaligned as usize).wrapping_add(16)
    );
}

#[test]
fn aggregates_use_consecutive_gr_slots_or_the_stack() {
    let mut frame = Frame::new(6);
    frame.save.gr[6] = 0x66;
    frame.save.gr[7] = 0x77;
    frame.stack[0] = 0x11;
    frame.stack[1] = 0x22;
    frame.wire();
    let mut reader = frame.reader();
    let mut out = [0u8; 32];
    // 16 bytes need two slots; gr_offs is -16, so both fit in the GR area.
    assert!(reader.next_aggregate_int(16, 8, &mut out));
    assert_eq!(u64::from_le_bytes(out[0..8].try_into().unwrap()), 0x66);
    assert_eq!(u64::from_le_bytes(out[8..16].try_into().unwrap()), 0x77);
    assert_eq!(reader.list().gr_offs, 0);
    // The next integer goes to the stack because NGRN was not reset.
    assert_eq!(reader.next_u64(), 0x11);
    // An aggregate after the GR area is exhausted: whole aggregate on the stack.
    assert!(reader.next_aggregate_int(12, 8, &mut out));
    assert_eq!(u64::from_le_bytes(out[0..8].try_into().unwrap()), 0x22);
    assert_eq!(reader.list().gr_offs, 0);
}

#[test]
fn a_partial_register_fit_sends_the_aggregate_to_the_stack() {
    let mut frame = Frame::new(7);
    frame.save.gr[7] = 0x77;
    frame.stack[0] = 0xAA;
    frame.stack[1] = 0xBB;
    frame.wire();
    let mut reader = frame.reader();
    let mut out = [0u8; 32];
    // One GR slot remains but 16 bytes need two: the whole aggregate goes to the stack and
    // gr_offs is set to 0 (NGRN := 8).
    assert!(reader.next_aggregate_int(16, 8, &mut out));
    assert_eq!(u64::from_le_bytes(out[0..8].try_into().unwrap()), 0xAA);
    assert_eq!(u64::from_le_bytes(out[8..16].try_into().unwrap()), 0xBB);
    assert_eq!(reader.list().gr_offs, 0);
    // The saved x7 was not consumed.
    assert_eq!(reader.next_u64(), 0);
}

#[test]
fn aggregates_larger_than_16_bytes_are_passed_by_reference() {
    let payload = [0x5Au8; 32];
    let mut frame = Frame::new(0);
    frame.save.gr[0] = payload.as_ptr() as u64;
    frame.wire();
    let mut reader = frame.reader();
    let mut out = [0u8; 32];
    assert!(reader.next_aggregate_int(32, 8, &mut out));
    assert_eq!(out, payload);
    assert_eq!(reader.list().gr_offs, -56);
}

#[test]
fn clone_and_no_write_back() {
    let mut frame = Frame::new(0);
    frame.save.gr = [1, 2, 3, 4, 5, 6, 7, 8];
    frame.stack[0] = 9;
    frame.wire();
    let before = frame.list;
    let mut first = frame.reader();
    assert_eq!(first.next_u64(), 1);
    let mut copy = first.clone_state();
    assert_eq!(copy.next_u64(), 2);
    assert_eq!(first.next_u64(), 2);
    for expected in 3..=8 {
        assert_eq!(copy.next_u64(), expected);
    }
    assert_eq!(copy.next_u64(), 9);
    assert_eq!(frame.list.gr_offs, before.gr_offs);
    assert_eq!(frame.list.stack, before.stack);
}
