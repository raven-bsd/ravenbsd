// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The only module of `raven-libc-malloc` allowed to contain `unsafe` (DR-39).
//!
//! All algorithm code works on `usize` addresses; every raw read, write, fill and copy goes through
//! this module. Each block states its precondition.

use core::cell::Cell;
use core::ptr;

/// A `Cell` that may be shared by threads when an external lock serialises access.
///
/// # Safety
/// The caller must ensure that mutations are serialised by the allocator's `Lock`; the type only
/// moves the `Sync` marker here, next to the audited raw access, as DR-39 requires.
pub struct SyncCell<T>(Cell<T>);

// SAFETY: every use in the allocator is under the backend's lock; `SyncCell` has no aliasing
// references alive across a call, and all values are `Copy`.
unsafe impl<T> Sync for SyncCell<T> {}

impl<T: Copy> SyncCell<T> {
    /// Create a shared cell.
    pub const fn new(value: T) -> Self {
        Self(Cell::new(value))
    }

    /// Read the current value.
    pub fn get(&self) -> T {
        self.0.get()
    }

    /// Replace the value.
    pub fn set(&self, value: T) {
        self.0.set(value);
    }
}

/// Read one byte.
///
/// # Safety
/// `addr` must point to at least one readable byte for the duration of the call.
#[must_use]
pub fn read_u8(addr: usize) -> u8 {
    // SAFETY: the caller guarantees the readable range.
    unsafe { ptr::read_unaligned(addr as *const u8) }
}

/// Read a little-endian `u16`.
///
/// # Safety
/// `addr` must point to at least two readable bytes.
#[must_use]
pub fn read_u16(addr: usize) -> u16 {
    // SAFETY: the caller guarantees the readable range.
    unsafe { ptr::read_unaligned(addr as *const u16) }
}

/// Read a little-endian `u32`.
///
/// # Safety
/// `addr` must point to at least four readable bytes.
#[must_use]
pub fn read_u32(addr: usize) -> u32 {
    // SAFETY: the caller guarantees the readable range.
    unsafe { ptr::read_unaligned(addr as *const u32) }
}

/// Read a little-endian `u64`.
///
/// # Safety
/// `addr` must point to at least eight readable bytes.
#[must_use]
pub fn read_u64(addr: usize) -> u64 {
    // SAFETY: the caller guarantees the readable range.
    unsafe { ptr::read_unaligned(addr as *const u64) }
}

/// Read a `usize`.
///
/// # Safety
/// `addr` must point to a readable `usize`.
#[must_use]
pub fn read_usize(addr: usize) -> usize {
    // SAFETY: the caller guarantees the readable range.
    unsafe { ptr::read_unaligned(addr as *const usize) }
}

/// Write one byte.
///
/// # Safety
/// `addr` must point to at least one writable byte.
pub fn write_u8(addr: usize, value: u8) {
    // SAFETY: the caller guarantees the writable range.
    unsafe { ptr::write_unaligned(addr as *mut u8, value) };
}

/// Write a little-endian `u16`.
///
/// # Safety
/// `addr` must point to at least two writable bytes.
pub fn write_u16(addr: usize, value: u16) {
    // SAFETY: the caller guarantees the writable range.
    unsafe { ptr::write_unaligned(addr as *mut u16, value) };
}

/// Write a little-endian `u32`.
///
/// # Safety
/// `addr` must point to at least four writable bytes.
pub fn write_u32(addr: usize, value: u32) {
    // SAFETY: the caller guarantees the writable range.
    unsafe { ptr::write_unaligned(addr as *mut u32, value) };
}

/// Write a little-endian `u64`.
///
/// # Safety
/// `addr` must point to at least eight writable bytes.
pub fn write_u64(addr: usize, value: u64) {
    // SAFETY: the caller guarantees the writable range.
    unsafe { ptr::write_unaligned(addr as *mut u64, value) };
}

/// Write a `usize`.
///
/// # Safety
/// `addr` must point to a writable `usize`.
pub fn write_usize(addr: usize, value: usize) {
    // SAFETY: the caller guarantees the writable range.
    unsafe { ptr::write_unaligned(addr as *mut usize, value) };
}

/// Fill `len` bytes with `value`.
///
/// # Safety
/// `addr..addr + len` must be one writable range.
pub fn fill(addr: usize, value: u8, len: usize) {
    // SAFETY: the caller guarantees the writable range.
    unsafe { ptr::write_bytes(addr as *mut u8, value, len) };
}

/// Copy `len` non-overlapping bytes.
///
/// # Safety
/// Both ranges must be readable/writable and must not overlap.
pub fn copy(dst: usize, src: usize, len: usize) {
    // SAFETY: the caller guarantees the ranges and the non-overlap.
    unsafe { ptr::copy_nonoverlapping(src as *const u8, dst as *mut u8, len) };
}

/// Copy `len` possibly overlapping bytes.
///
/// # Safety
/// Both ranges must be readable/writable for `len` bytes.
pub fn copy_overlapping(dst: usize, src: usize, len: usize) {
    // SAFETY: the caller guarantees both ranges; `ptr::copy` handles overlap.
    unsafe { ptr::copy(src as *const u8, dst as *mut u8, len) };
}

/// Compare `len` bytes; returns the sign of the first differing byte.
///
/// # Safety
/// Both ranges must be readable for `len` bytes.
#[must_use]
pub fn compare(a: usize, b: usize, len: usize) -> i32 {
    let mut index = 0usize;
    while index < len {
        let left = read_u8(a.wrapping_add(index));
        let right = read_u8(b.wrapping_add(index));
        if left != right {
            return i32::from(left) - i32::from(right);
        }
        index = index.wrapping_add(1);
    }
    0
}

/// Fill `len` bytes without letting the optimiser remove the stores.
///
/// # Safety
/// `addr..addr + len` must be one writable range.
pub fn fill_volatile(addr: usize, value: u8, len: usize) {
    let mut index = 0usize;
    while index < len {
        // SAFETY: the caller guarantees the writable range.
        unsafe { ptr::write_volatile(addr.wrapping_add(index) as *mut u8, value) };
        index = index.wrapping_add(1);
    }
}

/// Copy `data` into `addr`.
///
/// # Safety
/// `addr..addr + data.len()` must be writable.
pub fn write_bytes(addr: usize, data: &[u8]) {
    copy(addr, data.as_ptr() as usize, data.len());
}

/// Copy `len` bytes from `addr` into `out`.
///
/// # Safety
/// `addr..addr + len` must be readable and `out.len() == len`.
pub fn read_bytes(addr: usize, out: &mut [u8]) {
    copy(out.as_mut_ptr() as usize, addr, out.len());
}

#[cfg(test)]
#[path = "raw_ut.rs"]
mod tests;
