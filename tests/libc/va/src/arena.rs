// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
//! A byte arena addressed by raw pointers, with canaries and garbage injection.
//!
//! The ABI model builds the memory image a variadic stub would build inside one `Arena`;
//! readers are then pointed at it. The arena is a fixed-size `Vec<u8>` that never reallocates
//! after construction, so the absolute address returned by [`Arena::ptr`] is stable.

#![allow(dead_code)] // included by several test binaries, each using a subset

/// Fill byte of the two canaries that guard every arena.
pub const CANARY: u8 = 0xCC;

/// Garbage byte the model writes into padding (long doubles, aggregate padding).
pub const PADDING: u8 = 0xA5;

/// A fixed-size, 16-byte aligned byte arena.
pub struct Arena {
    words: Vec<u64>,
    len: usize,
}

impl Arena {
    /// A 16-byte aligned arena of `len` bytes, filled with [`CANARY`].
    #[must_use]
    pub fn new(len: usize) -> Self {
        let words = len.div_ceil(8);
        let mut storage = vec![0u64; words];
        // SAFETY: `storage` is a live, 8-byte aligned `u64` allocation; viewing it as bytes
        // is always valid and `words * 8 >= len`.
        let raw = unsafe {
            core::slice::from_raw_parts_mut(storage.as_mut_ptr().cast::<u8>(), words * 8)
        };
        for byte in raw.iter_mut() {
            *byte = CANARY;
        }
        Self {
            words: storage,
            len,
        }
    }

    /// The arena length in bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when the arena holds no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The absolute address of offset `off`.
    ///
    /// # Panics
    ///
    /// Panics when `off` is outside the arena.
    #[must_use]
    pub fn ptr(&self, off: usize) -> usize {
        assert!(
            off <= self.len,
            "offset {off} outside the {}-byte arena",
            self.len
        );
        self.words.as_ptr() as usize + off
    }

    /// Copy `data` to `off`.
    ///
    /// # Panics
    ///
    /// Panics when the write would leave the arena.
    pub fn write(&mut self, off: usize, data: &[u8]) {
        let end = off.checked_add(data.len()).expect("arena offset overflow");
        assert!(
            end <= self.len,
            "write of {} bytes at {off} leaves the arena",
            data.len()
        );
        // SAFETY: `off + data.len() <= len <= words * 8`, so the destination is inside the
        // live allocation and the regions cannot overlap (the source is not the arena).
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                (self.words.as_mut_ptr() as *mut u8).add(off),
                data.len(),
            );
        }
    }

    /// Fill `len` bytes at `off` with `value`.
    pub fn fill(&mut self, off: usize, len: usize, value: u8) {
        assert!(
            off.checked_add(len).is_some_and(|end| end <= self.len),
            "fill at {off} leaves the arena"
        );
        // SAFETY: the range is inside the live allocation.
        unsafe {
            core::ptr::write_bytes((self.words.as_mut_ptr() as *mut u8).add(off), value, len);
        }
    }

    /// The bytes at `off`.
    #[must_use]
    pub fn read(&self, off: usize, len: usize) -> &[u8] {
        assert!(
            off.checked_add(len).is_some_and(|end| end <= self.len),
            "read at {off} leaves the arena"
        );
        // SAFETY: the range is inside the live allocation, and the borrow of `self` keeps it
        // alive for the returned slice's lifetime.
        unsafe { core::slice::from_raw_parts((self.words.as_ptr() as *const u8).add(off), len) }
    }

    /// Store a little-endian `u64` at `off`.
    pub fn write_u64(&mut self, off: usize, value: u64) {
        self.write(off, &value.to_le_bytes());
    }

    /// Store a little-endian `u32` at `off`.
    pub fn write_u32(&mut self, off: usize, value: u32) {
        self.write(off, &value.to_le_bytes());
    }

    /// Store a little-endian `i32` at `off`.
    pub fn write_i32(&mut self, off: usize, value: i32) {
        self.write(off, &value.to_le_bytes());
    }

    /// Store an absolute pointer to `target` at `off`.
    pub fn write_ptr(&mut self, off: usize, target: usize) {
        self.write_u64(off, target as u64);
    }

    /// Load a little-endian `u32` from `off`.
    #[must_use]
    pub fn read_u32(&self, off: usize) -> u32 {
        let bytes = self.read(off, 4);
        let mut raw = [0u8; 4];
        raw.copy_from_slice(bytes);
        u32::from_le_bytes(raw)
    }

    /// Load a little-endian `u64` from `off`.
    #[must_use]
    pub fn read_u64(&self, off: usize) -> u64 {
        let bytes = self.read(off, 8);
        let mut raw = [0u8; 8];
        raw.copy_from_slice(bytes);
        u64::from_le_bytes(raw)
    }

    /// True when the 16 bytes before `off` are the untouched canary.
    #[must_use]
    pub fn before_canary_ok(&self, off: usize) -> bool {
        off >= 16 && self.read(off - 16, 16).iter().all(|byte| *byte == CANARY)
    }

    /// True when the 16 bytes after `off + len` are the untouched canary.
    #[must_use]
    pub fn after_canary_ok(&self, off: usize, len: usize) -> bool {
        self.read(off + len, 16).iter().all(|byte| *byte == CANARY)
    }
}

#[cfg(test)]
#[path = "arena_ut.rs"]
mod tests;
