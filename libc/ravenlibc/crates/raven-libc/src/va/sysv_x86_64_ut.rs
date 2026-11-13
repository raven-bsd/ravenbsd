// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! Unit tests of the x86_64 SysV layout and reader (`sysv_x86_64.rs`).

use super::*;
use crate::va::VaReaderExt;

/// A 16-byte aligned frame shaped like the stub's: save area, then the `va_list`.
#[repr(C, align(16))]
struct Frame {
    save: VaSaveSysV,
    list: VaListSysV,
    overflow: [u64; 16],
}

impl Frame {
    fn new(named: u32) -> Self {
        Self {
            save: VaSaveSysV {
                gp: [0; GP_SLOTS],
                xmm: [[0; 16]; 8],
            },
            list: VaListSysV {
                gp_offset: named.wrapping_mul(8),
                fp_offset: FP_OFFSET_INIT,
                overflow_arg_area: core::ptr::null_mut(),
                reg_save_area: core::ptr::null_mut(),
            },
            overflow: [0; 16],
        }
    }

    fn wire(&mut self) {
        self.list.reg_save_area = core::ptr::addr_of_mut!(self.save).cast::<u8>();
        self.list.overflow_arg_area = self.overflow.as_mut_ptr().cast::<u8>();
    }

    fn reader(&self) -> SysvReader {
        SysvReader::from_list(self.list)
    }
}

#[test]
fn layout_is_the_contract() {
    assert_eq!(core::mem::size_of::<VaListSysV>(), 24);
    assert_eq!(core::mem::align_of::<VaListSysV>(), 8);
    assert_eq!(core::mem::offset_of!(VaListSysV, gp_offset), 0);
    assert_eq!(core::mem::offset_of!(VaListSysV, fp_offset), 4);
    assert_eq!(core::mem::offset_of!(VaListSysV, overflow_arg_area), 8);
    assert_eq!(core::mem::offset_of!(VaListSysV, reg_save_area), 16);
    assert_eq!(core::mem::size_of::<VaSaveSysV>(), 176);
    assert_eq!(core::mem::align_of::<VaSaveSysV>(), 16);
}

#[test]
fn six_integers_use_gp_and_the_seventh_the_stack() {
    let mut frame = Frame::new(0);
    for (index, slot) in frame.save.gp.iter_mut().enumerate() {
        *slot = 0x1111_0000_0000_0000 | index as u64;
    }
    frame.overflow[0] = 0x7777;
    frame.wire();
    let mut reader = frame.reader();
    for index in 0..6 {
        assert_eq!(reader.next_u64(), 0x1111_0000_0000_0000 | index);
    }
    assert_eq!(reader.next_u64(), 0x7777);
}

#[test]
fn named_arguments_shift_gp_offset() {
    let mut frame = Frame::new(3);
    frame.save.gp[3] = 0xdead_beef;
    frame.overflow[0] = 0xfeed;
    frame.wire();
    let mut reader = frame.reader();
    assert_eq!(reader.list().gp_offset, 24);
    assert_eq!(reader.next_u64(), 0xdead_beef);
    for _ in 0..2 {
        assert_eq!(reader.next_u64(), 0);
    }
    assert_eq!(reader.next_u64(), 0xfeed);
}

#[test]
fn eight_doubles_use_the_low_half_of_the_xmm_slots() {
    let mut frame = Frame::new(0);
    for (index, slot) in frame.save.xmm.iter_mut().enumerate() {
        slot[..8].copy_from_slice(&(0x4000_0000_0000_0000u64 | index as u64).to_le_bytes());
        // fill the high half with garbage: the reader must ignore it
        for byte in &mut slot[8..] {
            *byte = 0xA5;
        }
    }
    frame.overflow[0] = 0x9999;
    frame.wire();
    let mut reader = frame.reader();
    for index in 0..8 {
        let bits = reader.next_f64_bits();
        if cfg!(raven_va_softfloat) {
            // The soft-float profile never reads a vector register.
            assert_eq!(bits, 0x9999);
        } else {
            assert_eq!(bits, 0x4000_0000_0000_0000 | index as u64);
        }
    }
    if !cfg!(raven_va_softfloat) {
        assert_eq!(reader.next_f64_bits(), 0x9999);
    }
}

#[test]
fn long_double_is_memory_class_and_16_aligned() {
    let mut frame = Frame::new(0);
    // A misaligned overflow pointer: the reader must align it up to 16.
    let base = frame.overflow.as_mut_ptr().cast::<u8>();
    // SAFETY: `base` points into the frame's own 128-byte array; adding 4 stays inside it,
    // and this offset is deliberately NOT 16-byte aligned (the frame fields make +8 aligned).
    let misaligned = unsafe { base.add(4) };
    frame.list.overflow_arg_area = misaligned;
    frame.list.reg_save_area = core::ptr::addr_of_mut!(frame.save).cast::<u8>();
    let value: u128 = 0x4000_c90f_daa2_2168_0000;
    // SAFETY: the aligned slot is inside `frame.overflow`.
    unsafe {
        core::ptr::write_unaligned(align_up_16(misaligned as usize) as *mut u64, value as u64);
        core::ptr::write_unaligned(
            (align_up_16(misaligned as usize) as *mut u8)
                .add(8)
                .cast::<u16>(),
            (value >> 64) as u16,
        );
    }
    let mut reader = SysvReader::from_list(frame.list);
    let bits = reader.next_ld_bits();
    assert_eq!(bits, value & 0xffff_ffff_ffff_ffff_ffff);
    // bits 80..128 are zero even when the 6 padding bytes are not.
    assert_eq!(bits >> 80, 0);
    // 16 bytes were consumed from the aligned slot.
    assert_eq!(
        reader.list().overflow_arg_area as usize,
        align_up_16(misaligned as usize).wrapping_add(16)
    );
}

#[test]
fn clone_state_is_va_copy() {
    let mut frame = Frame::new(0);
    frame.save.gp = [1, 2, 3, 4, 5, 6];
    frame.overflow[0] = 7;
    frame.wire();
    let mut first = frame.reader();
    assert_eq!(first.next_u64(), 1);
    let mut copy = first.clone_state();
    assert_eq!(first.next_u64(), 2);
    assert_eq!(copy.next_u64(), 2);
    assert_eq!(copy.next_u64(), 3);
    for expected in 4..=6 {
        assert_eq!(copy.next_u64(), expected);
    }
    assert_eq!(copy.next_u64(), 7);
}

#[test]
fn the_caller_list_bytes_are_never_written() {
    let mut frame = Frame::new(1);
    frame.save.gp = [0x10, 0x20, 0x30, 0x40, 0x50, 0x60];
    frame.wire();
    let before = frame.list;
    let mut reader = frame.reader();
    for _ in 0..8 {
        let _ = reader.next_u64();
    }
    assert_eq!(frame.list.gp_offset, before.gp_offset);
    assert_eq!(frame.list.overflow_arg_area, before.overflow_arg_area);
    assert_eq!(frame.list.reg_save_area, before.reg_save_area);
}

#[test]
fn aggregates_use_gp_slots_while_they_fit() {
    let mut frame = Frame::new(0);
    for (index, slot) in frame.save.gp.iter_mut().enumerate() {
        *slot = 0xA000_0000_0000_0000 | index as u64;
    }
    frame.overflow[0] = 0xB0;
    frame.overflow[1] = 0xB1;
    frame.wire();
    let mut reader = frame.reader();
    let mut out = [0u8; 32];
    assert!(reader.next_aggregate_int(16, 8, &mut out));
    assert_eq!(
        u64::from_le_bytes(out[0..8].try_into().unwrap()),
        0xA000_0000_0000_0000
    );
    assert_eq!(
        u64::from_le_bytes(out[8..16].try_into().unwrap()),
        0xA000_0000_0000_0001
    );
    assert_eq!(reader.list().gp_offset, 16);
    // 32 bytes do not fit in the serial register class: MEMORY, from the overflow area.
    assert!(reader.next_aggregate_int(32, 8, &mut out));
    assert_eq!(u64::from_le_bytes(out[0..8].try_into().unwrap()), 0xB0);
    assert_eq!(reader.list().gp_offset, 16);
    assert_eq!(
        reader.list().overflow_arg_area as usize,
        frame.overflow.as_ptr() as usize + 32
    );
}

#[test]
fn aggregates_outside_the_subset_consume_nothing() {
    let mut frame = Frame::new(0);
    frame.save.gp[0] = 0x1234;
    frame.wire();
    let mut reader = frame.reader();
    let mut out = [0u8; 32];
    assert!(!reader.next_aggregate_int(4, 4, &mut out));
    assert!(!reader.next_aggregate_int(8, 16, &mut out));
    assert!(!reader.next_aggregate_int(20, 8, &mut out));
    assert!(!reader.next_aggregate_int(64, 8, &mut out));
    assert_eq!(reader.next_u64(), 0x1234);
}
