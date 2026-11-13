// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! riscv64 psABI `va_list`: the contiguous save area and the cursor reader.
//!
//! CLEANROOM (`donor-map/04` 3.9.3, written from the riscv64 psABI variadic rules; no donor
//! file exists or was read for code). UNVERIFIED against a compiled caller: the non-`v`
//! forms are link-time failing stubs until a `qemu-user` run passes (H-16, RD-34).
//!
//! Variadic floating arguments use the INTEGER convention: a `double` travels in `a0-a7` or
//! a stack slot as 8 bytes and `fa*` registers are not used; a `long double` (binary128)
//! travels in an aligned (even-numbered) register pair or a 16-byte aligned stack slot. The
//! frame is 80 bytes: `ra` at 0, `a0..a7` at 16..80, so the save area ends exactly at the
//! incoming `sp` and is contiguous with the caller's stack arguments. `va_list` is a single
//! pointer (the cursor) passed BY VALUE.

use raven_libc_fmt::VaReader;

use crate::va::{VaReaderExt, aggregate_in_subset, align_up_8, align_up_16};

/// Number of saved integer argument registers.
pub const A_SLOTS: usize = 8;

/// Bytes of the saved `ra` and its padding.
pub const RA_PAD_BYTES: usize = 16;

/// Offset of the first saved argument register inside the frame.
pub const SAVE_OFFSET: usize = 16;

/// Size of the register save area in bytes.
pub const SAVE_SIZE: usize = 80;

/// Size of the stub frame in bytes.
pub const FRAME_SIZE: usize = 80;

/// The riscv64 `va_list`: a cursor into the contiguous save/stack area, passed by value.
pub type VaListRiscv64 = *mut u8;

/// The riscv64 register save area (80 bytes, 16-byte aligned; `a` ends at the entry `sp`).
#[repr(C, align(16))]
#[derive(Clone, Copy)]
pub struct VaSaveRiscv64 {
    /// Saved `ra` plus one padding word.
    pub _ra_pad: [u64; 2],
    /// Saved `a0..a7`.
    pub a: [u64; A_SLOTS],
}

const _: () = assert!(core::mem::size_of::<VaSaveRiscv64>() == SAVE_SIZE);
const _: () = assert!(core::mem::align_of::<VaSaveRiscv64>() == 16);
const _: () = assert!(core::mem::offset_of!(VaSaveRiscv64, a) == SAVE_OFFSET);
const _: () = assert!(FRAME_SIZE == SAVE_SIZE);
const _: () = assert!(SAVE_OFFSET + A_SLOTS * 8 == SAVE_SIZE);

/// A reader over one riscv64 `va_list` cursor.
///
/// The cursor is copied on construction (RD-new 4); the caller's `va_list` is a by-value
/// pointer and is never written to. [`VaReader::clone_state`] copies the cursor.
pub struct RiscvReader {
    cursor: *mut u8,
}

impl RiscvReader {
    /// Wrap the `void *` a `v*` function receives by value.
    ///
    /// # Safety
    ///
    /// `ap` must point into a live contiguous save/stack argument area that stays readable
    /// for as long as the reader and its clones are used.
    pub unsafe fn from_cursor(ap: VaListRiscv64) -> Self {
        debug_assert!(!ap.is_null(), "a v* function never receives a NULL va_list");
        Self { cursor: ap }
    }

    /// The cursor the reader currently holds.
    #[must_use]
    pub const fn cursor(&self) -> VaListRiscv64 {
        self.cursor
    }
}

impl VaReader for RiscvReader {
    fn next_u64(&mut self) -> u64 {
        let slot = self.cursor;
        // SAFETY: the caller supplied one 8-byte slot at the cursor.
        unsafe {
            let value = core::ptr::read_unaligned(slot.cast::<u64>());
            self.cursor = slot.add(8);
            value
        }
    }

    fn next_f64_bits(&mut self) -> u64 {
        // Variadic floating arguments use the integer convention on riscv64.
        self.next_u64()
    }

    fn next_ld_bits(&mut self) -> u128 {
        let slot = align_up_16(self.cursor as usize) as *mut u8;
        // SAFETY: a binary128 stack argument occupies a 16-byte aligned 16-byte slot.
        unsafe {
            let value = core::ptr::read_unaligned(slot.cast::<u128>());
            self.cursor = slot.add(16);
            value
        }
    }

    fn clone_state(&self) -> Self {
        Self {
            cursor: self.cursor,
        }
    }
}

impl VaReaderExt for RiscvReader {
    fn next_aggregate_int(&mut self, size: usize, align: usize, out: &mut [u8; 32]) -> bool {
        if !aggregate_in_subset(size, align) {
            return false;
        }
        let Some(destination) = out.get_mut(..size) else {
            return false;
        };
        if size <= 16 {
            // A split between the last register and the stack is automatically contiguous
            // in this ABI: the save area ends exactly at the entry `sp`.
            // SAFETY: the caller supplied `round_up(size, 8)` bytes at the cursor.
            unsafe {
                core::ptr::copy_nonoverlapping(self.cursor, destination.as_mut_ptr(), size);
                self.cursor = self.cursor.add(align_up_8(size));
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
#[path = "riscv64_ut.rs"]
mod tests;
