// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! aarch64 AAPCS64 `va_list`: the frame, the two register areas and the reader.
//!
//! CLEANROOM (`donor-map/04` 3.9.2, written from the AAPCS64 B.5 `va_arg` algorithm; no
//! donor file exists or was read for code). UNVERIFIED against a compiled caller: the
//! non-`v` forms are link-time failing stubs until a `qemu-user` run passes (H-16, RD-34).
//!
//! Frame (240 bytes = 15 x 16, offsets from `sp` after the prologue): `[0..16)` saved
//! `x29 x30`, `[16..80)` `x0..x7` (the GR area), `[80..208)` `q0..q7` (the VR area) and the
//! `va_list` at `[208..240)`. The list points at the entry `sp` (`sp+240`, the first stack
//! argument), the ends of both register areas and the two negative offsets.
//!
//! The Apple ABI is not used: Apple passes every variadic argument on the stack.

use raven_libc_fmt::VaReader;

use crate::va::{VaReaderExt, aggregate_in_subset, align_up_16};

/// Bytes of the saved frame pointer and link register.
pub const SAVE_FPLR: usize = 16;

/// Bytes of the GR area: 8 x 8.
pub const GR_BYTES: usize = 64;

/// Offset of the GR area inside the frame.
pub const GR_OFFSET: usize = 16;

/// The end of the GR area (the `gr_top` of the list).
pub const GR_TOP: usize = 80;

/// Bytes of the VR area: 8 x 16.
pub const VR_BYTES: usize = 128;

/// Offset of the VR area inside the frame.
pub const VR_OFFSET: usize = 80;

/// The end of the VR area (the `vr_top` of the list).
pub const VR_TOP: usize = 208;

/// Size of the register save area (GR + VR, without the list).
pub const SAVE_SIZE: usize = 208;

/// Size of the stub frame in bytes.
pub const FRAME_SIZE: usize = 240;

/// Offset of the `va_list` inside the stub frame.
pub const LIST_OFFSET: usize = 208;

/// The initial `vr_offs` of the hard-float stub: no named vector argument exists.
pub const VR_OFFSET_INIT: i32 = -128;

/// The `vr_offs` of the soft-float stub: every float read falls to the stack.
pub const VR_OFFSET_SOFTFLOAT: i32 = 0;

/// The aarch64 AAPCS64 `va_list` (32 bytes, align 8; passed by invisible reference).
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VaListAapcs64 {
    /// The first stack argument (the entry `sp` of the stub).
    pub stack: *mut u8,
    /// The end of the GR area.
    pub gr_top: *mut u8,
    /// The end of the VR area.
    pub vr_top: *mut u8,
    /// Negative byte offset of the next free GR slot; `>= 0` means "stack only".
    pub gr_offs: i32,
    /// Negative byte offset of the next free VR slot; must be a multiple of 16.
    pub vr_offs: i32,
}

/// The aarch64 register save area (208 bytes, 16-byte aligned).
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct VaSaveAapcs64 {
    /// Saved `x29 x30`.
    pub _fplr: [u64; 2],
    /// Saved `x0..x7`.
    pub gr: [u64; 8],
    /// Saved `q0..q7`, 16 bytes each; unused by the soft-float profile.
    pub vr: [[u8; 16]; 8],
}

const _: () = assert!(core::mem::size_of::<VaListAapcs64>() == 32);
const _: () = assert!(core::mem::align_of::<VaListAapcs64>() == 8);
const _: () = assert!(core::mem::offset_of!(VaListAapcs64, stack) == 0);
const _: () = assert!(core::mem::offset_of!(VaListAapcs64, gr_top) == 8);
const _: () = assert!(core::mem::offset_of!(VaListAapcs64, vr_top) == 16);
const _: () = assert!(core::mem::offset_of!(VaListAapcs64, gr_offs) == 24);
const _: () = assert!(core::mem::offset_of!(VaListAapcs64, vr_offs) == 28);
const _: () = assert!(core::mem::size_of::<VaSaveAapcs64>() == SAVE_SIZE);
const _: () = assert!(core::mem::align_of::<VaSaveAapcs64>() == 16);
const _: () = assert!(GR_TOP == GR_OFFSET + GR_BYTES);
const _: () = assert!(VR_TOP == GR_TOP + VR_BYTES);
const _: () = assert!(LIST_OFFSET == VR_TOP);
const _: () = assert!(FRAME_SIZE == LIST_OFFSET + core::mem::size_of::<VaListAapcs64>());

/// A reader over one aarch64 AAPCS64 `va_list`.
///
/// The reader owns a copy of the 32-byte list (RD-new 4); [`VaReader::clone_state`] copies it
/// (`va_copy`).
pub struct Aapcs64Reader {
    list: VaListAapcs64,
}

impl Aapcs64Reader {
    /// Copy the caller's `va_list`, as a `v*` function sees it (passed by invisible
    /// reference).
    ///
    /// # Safety
    ///
    /// `ap` must point to a readable [`VaListAapcs64`] whose `stack`, `gr_top` and `vr_top`
    /// stay readable for as long as the reader and its clones are used.
    pub unsafe fn from_ptr(ap: *mut VaListAapcs64) -> Self {
        debug_assert!(!ap.is_null(), "a v* function never receives a NULL va_list");
        // SAFETY: the caller guarantees that `ap` is readable for 32 bytes.
        let list = unsafe { core::ptr::read_unaligned(ap) };
        Self { list }
    }

    /// A reader over an owned copy of `list` (tests and recorder targets).
    #[must_use]
    pub const fn from_list(list: VaListAapcs64) -> Self {
        Self { list }
    }

    /// A copy of the list the reader currently holds.
    #[must_use]
    pub const fn list(&self) -> VaListAapcs64 {
        self.list
    }

    /// The next GR slot: a register while one fits, then the stack (B.5).
    fn next_gr(&mut self) -> *mut u8 {
        let list = &mut self.list;
        let offset = list.gr_offs;
        if offset < 0 {
            let next = offset.wrapping_add(8);
            list.gr_offs = next;
            if next <= 0 {
                // SAFETY: `offset` is negative and `offset + 8 <= 0`, so the 8-byte slot at
                // `gr_top + offset` lies inside the saved GR area.
                return unsafe { list.gr_top.offset(offset as isize) };
            }
        }
        // The stack path: `gr_offs >= 0` or the register overflowed into the stack.
        let slot = list.stack;
        // SAFETY: the caller supplied one 8-byte stack slot for this argument.
        list.stack = unsafe { slot.add(8) };
        slot
    }

    /// The next VR slot: a register while one fits, then the stack (B.5).
    fn next_vr(&mut self) -> *mut u8 {
        let list = &mut self.list;
        let offset = list.vr_offs;
        if offset < 0 {
            let next = offset.wrapping_add(16);
            list.vr_offs = next;
            if next <= 0 {
                // SAFETY: `offset` is negative and `offset + 16 <= 0`, so the 16-byte slot
                // at `vr_top + offset` lies inside the saved VR area.
                return unsafe { list.vr_top.offset(offset as isize) };
            }
        }
        let slot = list.stack;
        // SAFETY: the caller supplied one 8-byte stack slot for this argument.
        list.stack = unsafe { slot.add(8) };
        slot
    }

    /// The stack slot of a 16-byte aligned `long double`, with the B.5 alignment rule.
    fn next_stack_ld(&mut self) -> *mut u8 {
        let list = &mut self.list;
        let slot = align_up_16(list.stack as usize) as *mut u8;
        // SAFETY: the caller supplied a 16-byte aligned 16-byte stack slot.
        list.stack = unsafe { slot.add(16) };
        slot
    }
}

impl VaReader for Aapcs64Reader {
    fn next_u64(&mut self) -> u64 {
        let slot = self.next_gr();
        // SAFETY: `next_gr` returns a readable 8-byte slot inside the live frame.
        unsafe { core::ptr::read_unaligned(slot.cast::<u64>()) }
    }

    fn next_f64_bits(&mut self) -> u64 {
        let slot = self.next_vr();
        // SAFETY: the low 8 bytes of the 16-byte VR slot (or an 8-byte stack slot) are
        // readable and hold the little-endian binary64 image.
        unsafe { core::ptr::read_unaligned(slot.cast::<u64>()) }
    }

    fn next_ld_bits(&mut self) -> u128 {
        let offset = self.list.vr_offs;
        let slot = if offset < 0 {
            let next = offset.wrapping_add(16);
            self.list.vr_offs = next;
            if next <= 0 {
                // SAFETY: the 16-byte VR slot at `vr_top + offset` is inside the VR area.
                unsafe { self.list.vr_top.offset(offset as isize) }
            } else {
                self.next_stack_ld()
            }
        } else {
            self.next_stack_ld()
        };
        // SAFETY: `slot` is a readable 16-byte slot holding the binary128 image.
        unsafe { core::ptr::read_unaligned(slot.cast::<u128>()) }
    }

    fn clone_state(&self) -> Self {
        Self { list: self.list }
    }
}

impl VaReaderExt for Aapcs64Reader {
    fn next_aggregate_int(&mut self, size: usize, align: usize, out: &mut [u8; 32]) -> bool {
        if !aggregate_in_subset(size, align) {
            return false;
        }
        let Some(destination) = out.get_mut(..size) else {
            return false;
        };
        if size <= 16 {
            let slots = size.div_ceil(8);
            let need = (slots as i32).wrapping_mul(8);
            let list = &mut self.list;
            let fits = list
                .gr_offs
                .checked_add(need)
                .is_some_and(|end| list.gr_offs < 0 && end <= 0);
            if fits {
                // SAFETY: `slots` whole 8-byte GR slots lie inside the saved GR area.
                unsafe {
                    let source = list.gr_top.offset(list.gr_offs as isize);
                    core::ptr::copy_nonoverlapping(source, destination.as_mut_ptr(), size);
                    list.gr_offs = list.gr_offs.wrapping_add(need);
                }
                return true;
            }
            // NGRN := 8: this aggregate and every later integer argument go to the stack.
            list.gr_offs = 0;
            let stack = list.stack;
            // SAFETY: the caller supplied `slots` 8-byte stack slots for the aggregate.
            unsafe {
                core::ptr::copy_nonoverlapping(stack, destination.as_mut_ptr(), size);
                list.stack = stack.add(slots.wrapping_mul(8));
            }
            return true;
        }
        // Composites larger than 16 bytes are passed by reference.
        let pointer = self.next_u64() as usize as *const u8;
        if pointer.is_null() {
            return false;
        }
        // SAFETY: the caller passed a pointer to a `size`-byte object it owns.
        unsafe { core::ptr::copy_nonoverlapping(pointer, destination.as_mut_ptr(), size) };
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
#[path = "aapcs64_ut.rs"]
mod tests;
