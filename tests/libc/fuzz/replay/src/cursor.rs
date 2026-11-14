// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! A structured view of the input bytes without any dependency.
//!
//! Out-of-data reads return zero (or an empty slice), so a short input is a legal input.

/// A little-endian cursor over arbitrary bytes.
#[derive(Clone, Debug)]
pub struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    /// Start at the beginning.
    #[must_use]
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    /// The next byte, or 0 at the end.
    pub fn u8(&mut self) -> u8 {
        let value = self.data.get(self.pos).copied().unwrap_or(0);
        self.pos = self.pos.saturating_add(1);
        value
    }

    /// The next two bytes little-endian; missing bytes are zero.
    pub fn u16(&mut self) -> u16 {
        u16::from_le_bytes([self.u8(), self.u8()])
    }

    /// The next four bytes little-endian; missing bytes are zero.
    pub fn u32(&mut self) -> u32 {
        u32::from_le_bytes([self.u8(), self.u8(), self.u8(), self.u8()])
    }

    /// The next eight bytes little-endian; missing bytes are zero.
    pub fn u64(&mut self) -> u64 {
        u64::from_le_bytes([
            self.u8(),
            self.u8(),
            self.u8(),
            self.u8(),
            self.u8(),
            self.u8(),
            self.u8(),
            self.u8(),
        ])
    }

    /// Up to `n` bytes; shorter at the end.
    pub fn take(&mut self, n: usize) -> &'a [u8] {
        let end = self.pos.saturating_add(n).min(self.data.len());
        let slice = self.data.get(self.pos..end).unwrap_or(&[]);
        self.pos = end;
        slice
    }

    /// Everything that is left.
    #[must_use]
    pub fn rest(&self) -> &'a [u8] {
        self.data.get(self.pos..).unwrap_or(&[])
    }

    /// One value from `items`; a zero byte selects the first item when `items` is empty only
    /// in the caller's fallback. The caller must pass a non-empty slice.
    ///
    /// # Panics
    ///
    /// Panics on an empty slice: a body's option table is never empty.
    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        let index = usize::from(self.u8()) % items.len();
        items[index]
    }

    /// One boolean from the low bit of the next byte.
    pub fn bool(&mut self) -> bool {
        self.u8() & 1 != 0
    }

    /// How many bytes are left.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
}

#[cfg(test)]
#[path = "cursor_ut.rs"]
mod tests;
