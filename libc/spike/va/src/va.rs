// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The SysV `va_list` view and its register-save-area reader.

/// The x86-64 SysV `va_list` object built by the trampoline.
#[repr(C)]
pub struct VaList {
    gp_offset: u32,
    fp_offset: u32,
    overflow_arg_area: *mut u8,
    reg_save_area: *mut u8,
}

/// A reader over a [`VaList`].
///
/// The list must remain valid for the lifetime of this reader. Integer arguments are taken from
/// the six GP save slots (offsets 0..48) and then from the overflow area; `double`s are taken from
/// the eight 16-byte XMM slots (offsets 48..176) and then from the overflow area.
pub struct VaReader {
    list: *mut VaList,
}

impl VaReader {
    /// Create a reader for a list built by the variadic trampoline.
    ///
    /// # Safety
    ///
    /// `list` must point to a live `VaList` whose `reg_save_area` and `overflow_arg_area` refer to
    /// readable argument storage for as long as the returned reader is used.
    pub unsafe fn new(list: *mut VaList) -> Self {
        Self { list }
    }

    /// Read the next integer argument.
    pub fn arg_u64(&mut self) -> u64 {
        // SAFETY: `next_gp` returns a readable slot in the live save area.
        let pointer = unsafe { self.next_gp() };
        // SAFETY: `next_gp` returns a readable 8-byte slot inside the live save/overflow area.
        unsafe { core::ptr::read_unaligned(pointer.cast::<u64>()) }
    }

    /// Read the next signed integer argument.
    pub fn arg_i64(&mut self) -> i64 {
        self.arg_u64() as i64
    }

    /// Read the next pointer argument.
    pub fn arg_ptr<T>(&mut self) -> *const T {
        self.arg_u64() as usize as *const T
    }

    /// Read the next `double` argument.
    pub fn arg_f64(&mut self) -> f64 {
        // SAFETY: `next_fp` returns a readable slot in the live save area.
        let pointer = unsafe { self.next_fp() };
        // SAFETY: `next_fp` returns a readable 8-byte slot inside the live save/overflow area.
        let bits = unsafe { core::ptr::read_unaligned(pointer.cast::<u64>()) };
        f64::from_bits(bits)
    }

    unsafe fn next_gp(&mut self) -> *mut u8 {
        // SAFETY: the constructor requires `self.list` to be valid for this reader.
        let list = unsafe { &mut *self.list };
        if list.gp_offset < 48 {
            // SAFETY: the offset is below the 48-byte GP save area.
            let pointer = unsafe { list.reg_save_area.add(list.gp_offset as usize) };
            list.gp_offset = list.gp_offset.saturating_add(8);
            pointer
        } else {
            let pointer = list.overflow_arg_area;
            // SAFETY: the ABI guarantees the overflow area has enough slots for the arguments the
            // caller supplied; advancing by one slot follows the ABI.
            list.overflow_arg_area = unsafe { pointer.add(8) };
            pointer
        }
    }

    unsafe fn next_fp(&mut self) -> *mut u8 {
        // SAFETY: the constructor requires `self.list` to be valid for this reader.
        let list = unsafe { &mut *self.list };
        if list.fp_offset < 176 {
            // SAFETY: the offset is below the 176-byte register save area.
            let pointer = unsafe { list.reg_save_area.add(list.fp_offset as usize) };
            list.fp_offset = list.fp_offset.saturating_add(16);
            pointer
        } else {
            let pointer = list.overflow_arg_area;
            // SAFETY: stack doubles occupy one 8-byte slot.
            list.overflow_arg_area = unsafe { pointer.add(8) };
            pointer
        }
    }
}
