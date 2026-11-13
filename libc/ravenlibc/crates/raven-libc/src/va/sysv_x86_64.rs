// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! x86_64 System V `va_list`: the register save area, the stub frame and the reader.
//!
//! Derived from the project's own `libc/spike/va` (K03, same author, BSD-3-Clause) per C59
//! section 5 row 1 and the `internal-copy` rule of `17-crate-layout.md` section 12: the
//! reader logic is kept, the type becomes owning (it copies the list), `next_ld_bits`,
//! `next_aggregate_int` and the soft-float rule are added, and no `f64` remains.
//!
//! Hosted layout (`donor-map/04` 3.9.1): the save area is 176 bytes, `[0..48)` the six
//! integer registers `rdi rsi rdx rcx r8 r9` and `[48..176)` the eight 16-byte XMM slots.
//! The stub frame is 208 bytes from the incoming `rsp` and the `va_list` sits at 176.
//!
//! Soft-float profile (`cfg(raven_va_softfloat)`, H-20/CR-21): the stub keeps the same
//! frame, does not touch `xmm`, stores `fp_offset = 176` and never tests `%al`; the reader
//! takes every `double` from the overflow area, because a soft-float caller passes it on the
//! stack (OBSERVED with clang, C59 section 2.6(b); RD-new 1).

use raven_libc_fmt::VaReader;

use crate::va::{VaReaderExt, aggregate_in_subset, align_up_8, align_up_16};

/// Number of integer argument slots of the save area.
pub const GP_SLOTS: usize = 6;

/// Bytes of the integer part of the save area: 6 x 8.
pub const GP_BYTES: usize = 48;

/// The integer part as a `u32`, the type of `gp_offset`.
pub const GP_BYTES_U32: u32 = 48;

/// Bytes of the vector part of the save area: 8 x 16.
pub const FP_BYTES: usize = 176;

/// The vector part as a `u32`, the type of `fp_offset`.
pub const FP_BYTES_U32: u32 = 176;

/// Total size of the register save area in bytes.
pub const SAVE_SIZE: usize = 176;

/// Size of the stub frame in bytes (`push %rbp`, `sub $208,%rsp`).
pub const FRAME_SIZE: usize = 208;

/// Offset of the `va_list` inside the stub frame.
pub const LIST_OFFSET: usize = 176;

/// Initial `fp_offset` of the hosted stub: no named floating argument exists.
pub const FP_OFFSET_INIT: u32 = 48;

/// Initial `fp_offset` of the soft-float stub: every float read falls to the overflow area.
pub const FP_OFFSET_SOFTFLOAT: u32 = 176;

/// The x86_64 System V `va_list` object (array element type in C; 24 bytes, align 8).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VaListSysV {
    /// Byte offset of the next integer register slot, `0..=48`.
    pub gp_offset: u32,
    /// Byte offset of the next vector register slot, `48..=176`.
    pub fp_offset: u32,
    /// The first stack argument (`rbp+16` in the stub).
    pub overflow_arg_area: *mut u8,
    /// The base of the register save area (`rsp` in the stub).
    pub reg_save_area: *mut u8,
}

/// The x86_64 register save area the stub spills into (176 bytes, 16-byte aligned).
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct VaSaveSysV {
    /// `rdi rsi rdx rcx r8 r9`.
    pub gp: [u64; GP_SLOTS],
    /// `xmm0..xmm7`, 16 bytes each; unused by the soft-float profile.
    pub xmm: [[u8; 16]; 8],
}

const _: () = assert!(core::mem::size_of::<VaListSysV>() == 24);
const _: () = assert!(core::mem::align_of::<VaListSysV>() == 8);
const _: () = assert!(core::mem::offset_of!(VaListSysV, gp_offset) == 0);
const _: () = assert!(core::mem::offset_of!(VaListSysV, fp_offset) == 4);
const _: () = assert!(core::mem::offset_of!(VaListSysV, overflow_arg_area) == 8);
const _: () = assert!(core::mem::offset_of!(VaListSysV, reg_save_area) == 16);
const _: () = assert!(core::mem::size_of::<VaSaveSysV>() == SAVE_SIZE);
const _: () = assert!(core::mem::align_of::<VaSaveSysV>() == 16);
const _: () = assert!(FRAME_SIZE.is_multiple_of(16));
const _: () = assert!(LIST_OFFSET + core::mem::size_of::<VaListSysV>() == 200);

/// A reader over one x86_64 System V `va_list`.
///
/// The reader owns a copy of the 24-byte list and never writes back into the caller's
/// `va_list` (RD-new 4); [`VaReader::clone_state`] copies it (`va_copy`).
pub struct SysvReader {
    list: VaListSysV,
}

impl SysvReader {
    /// Copy the caller's `va_list`, as a `v*` function sees it.
    ///
    /// # Safety
    ///
    /// `ap` must point to a readable, aligned-or-unaligned [`VaListSysV`] whose
    /// `reg_save_area` and `overflow_arg_area` stay readable for as long as the reader and
    /// its clones are used.
    pub unsafe fn from_ptr(ap: *mut VaListSysV) -> Self {
        debug_assert!(!ap.is_null(), "a v* function never receives a NULL va_list");
        // SAFETY: the caller guarantees that `ap` is readable for 24 bytes.
        let list = unsafe { core::ptr::read_unaligned(ap) };
        Self { list }
    }

    /// A reader over an owned copy of `list` (tests and recorder targets).
    #[must_use]
    pub const fn from_list(list: VaListSysV) -> Self {
        Self { list }
    }

    /// A copy of the list the reader currently holds.
    #[must_use]
    pub const fn list(&self) -> VaListSysV {
        self.list
    }

    /// The next 8-byte integer slot: a GP register slot while one remains, then the overflow
    /// area.
    fn next_gp(&mut self) -> *mut u8 {
        let list = &mut self.list;
        let offset = list.gp_offset;
        if offset < GP_BYTES_U32 {
            // SAFETY: `offset` is below the 48-byte GP area of the register save area the
            // caller's ABI built.
            let slot = unsafe { list.reg_save_area.add(offset as usize) };
            list.gp_offset = offset.wrapping_add(8);
            slot
        } else {
            let slot = list.overflow_arg_area;
            // SAFETY: the ABI guarantees one more 8-byte stack slot for this argument; the
            // caller's frame stays alive for the whole `v*` call.
            list.overflow_arg_area = unsafe { slot.add(8) };
            slot
        }
    }

    /// The next `double` slot: a 16-byte vector slot's low 8 bytes while one remains, then
    /// the overflow area. Under `cfg(raven_va_softfloat)` doubles only ever come from the
    /// overflow area (RD-new 1).
    #[cfg(not(raven_va_softfloat))]
    fn next_fp(&mut self) -> *mut u8 {
        let list = &mut self.list;
        let offset = list.fp_offset;
        if offset < FP_BYTES_U32 {
            // SAFETY: `offset` is below the 176-byte save area; the low 8 bytes of the
            // 16-byte slot are readable.
            let slot = unsafe { list.reg_save_area.add(offset as usize) };
            list.fp_offset = offset.wrapping_add(16);
            slot
        } else {
            let slot = list.overflow_arg_area;
            // SAFETY: a stack `double` occupies one 8-byte slot of the caller's frame.
            list.overflow_arg_area = unsafe { slot.add(8) };
            slot
        }
    }

    /// The next `double` slot of the soft-float profile: always the overflow area.
    #[cfg(raven_va_softfloat)]
    fn next_fp(&mut self) -> *mut u8 {
        let slot = self.list.overflow_arg_area;
        // SAFETY: a soft-float caller passes every `double` as one 8-byte stack slot.
        self.list.overflow_arg_area = unsafe { slot.add(8) };
        slot
    }
}

impl VaReader for SysvReader {
    fn next_u64(&mut self) -> u64 {
        let slot = self.next_gp();
        // SAFETY: `next_gp` returns a readable 8-byte slot inside the live frame.
        unsafe { core::ptr::read_unaligned(slot.cast::<u64>()) }
    }

    fn next_f64_bits(&mut self) -> u64 {
        let slot = self.next_fp();
        // SAFETY: `next_fp` returns a readable 8-byte slot inside the live frame.
        unsafe { core::ptr::read_unaligned(slot.cast::<u64>()) }
    }

    fn next_ld_bits(&mut self) -> u128 {
        let list = &mut self.list;
        let slot = align_up_16(list.overflow_arg_area as usize) as *mut u8;
        // SAFETY: a `long double` is memory class: the caller provided a 16-byte aligned
        // 16-byte slot in its stack argument area. Ten bytes are the value; the other six
        // are padding and read as zero here.
        // SAFETY: as above: the ten meaningful bytes, the six padding bytes zero-extended.
        unsafe {
            let low = core::ptr::read_unaligned(slot.cast::<u64>());
            let high = core::ptr::read_unaligned(slot.add(8).cast::<u16>());
            list.overflow_arg_area = slot.add(16);
            u128::from(low) | (u128::from(high) << 64)
        }
    }

    fn clone_state(&self) -> Self {
        Self { list: self.list }
    }
}

impl VaReaderExt for SysvReader {
    fn next_aggregate_int(&mut self, size: usize, align: usize, out: &mut [u8; 32]) -> bool {
        if !aggregate_in_subset(size, align) {
            return false;
        }
        let Some(destination) = out.get_mut(..size) else {
            return false;
        };
        let slots = size.div_ceil(8);
        if size <= 16 {
            let need = (slots as u32).wrapping_mul(8);
            let list = &mut self.list;
            if let Some(limit) = GP_BYTES_U32.checked_sub(need)
                && list.gp_offset <= limit
            {
                // SAFETY: `gp_offset + need <= 48`, so the whole aggregate is inside the
                // integer part of the save area.
                unsafe {
                    let source = list.reg_save_area.add(list.gp_offset as usize);
                    core::ptr::copy_nonoverlapping(source, destination.as_mut_ptr(), size);
                    list.gp_offset = list.gp_offset.wrapping_add(need);
                }
                return true;
            }
            // The WHOLE aggregate comes from the stack; `gp_offset` does not move.
            // SAFETY: the caller supplied `slots` 8-byte stack slots for the aggregate.
            unsafe {
                let source = list.overflow_arg_area;
                core::ptr::copy_nonoverlapping(source, destination.as_mut_ptr(), size);
                list.overflow_arg_area = source.add(slots.wrapping_mul(8));
            }
            return true;
        }
        // MEMORY class: always on the stack, rounded up to 8 bytes.
        let bytes = align_up_8(size);
        // SAFETY: the caller supplied `round_up(size, 8)` bytes of stack argument area.
        unsafe {
            let source = self.list.overflow_arg_area;
            core::ptr::copy_nonoverlapping(source, destination.as_mut_ptr(), size);
            self.list.overflow_arg_area = source.add(bytes);
        }
        true
    }
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::arithmetic_side_effects
)]
#[path = "sysv_x86_64_ut.rs"]
mod tests;
