// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The only way this crate touches an image: a bounds-checked byte view.
//!
//! Addresses are absolute virtual addresses (the load bias has already been added). A view never
//! merges neighbouring ranges: a read that would cross two mapped ranges is `None`, because the
//! loader could not perform it either. `len == 0` is accepted at an address that lies inside a
//! range or exactly at its end.

use alloc::vec::Vec;

/// A read-only view of the loaded image.
pub trait MemView {
    /// The bytes at `[vaddr, vaddr + len)`, or `None` unless the range lies inside one mapped
    /// range. A zero-length read is `Some(&[])` when `vaddr` is inside a range or at its end.
    fn read(&self, vaddr: u64, len: usize) -> Option<&[u8]>;

    /// The bytes before the first NUL at or after `vaddr`, or `None` when no NUL occurs inside
    /// the same range.
    fn cstr(&self, vaddr: u64) -> Option<&[u8]>;
}

/// One contiguous range of image memory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SliceMem<'a> {
    /// Absolute virtual address of `bytes[0]`.
    pub base: u64,
    /// The bytes.
    pub bytes: &'a [u8],
}

impl<'a> SliceMem<'a> {
    /// A range at `base`.
    pub fn new(base: u64, bytes: &'a [u8]) -> Self {
        Self { base, bytes }
    }

    /// The offset of `vaddr` inside the range, allowing the end of a zero-length read.
    fn offset(&self, vaddr: u64, len: usize) -> Option<usize> {
        let end_address = vaddr.checked_add(u64::try_from(len).ok()?)?;
        let self_end = self
            .base
            .checked_add(u64::try_from(self.bytes.len()).ok()?)?;
        if vaddr < self.base || end_address > self_end {
            return None;
        }
        let start = usize::try_from(vaddr.checked_sub(self.base)?).ok()?;
        let end = start.checked_add(len)?;
        self.bytes.get(start..end).map(|_| start)
    }
}

impl MemView for SliceMem<'_> {
    fn read(&self, vaddr: u64, len: usize) -> Option<&[u8]> {
        let start = self.offset(vaddr, len)?;
        let end = start.checked_add(len)?;
        self.bytes.get(start..end)
    }

    fn cstr(&self, vaddr: u64) -> Option<&[u8]> {
        let start = self.offset(vaddr, 0)?;
        let rest = self.bytes.get(start..)?;
        let end = rest.iter().position(|byte| *byte == 0)?;
        rest.get(..end)
    }
}

/// A set of ranges; the range that contains the address answers. Ranges never merge.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemSet<'a> {
    /// The ranges, in no particular order.
    pub ranges: Vec<SliceMem<'a>>,
}

impl<'a> MemSet<'a> {
    /// An empty set.
    pub fn new() -> Self {
        Self { ranges: Vec::new() }
    }

    /// Add one range.
    pub fn push(&mut self, range: SliceMem<'a>) {
        self.ranges.push(range);
    }
}

impl MemView for MemSet<'_> {
    fn read(&self, vaddr: u64, len: usize) -> Option<&[u8]> {
        self.ranges.iter().find_map(|range| {
            range
                .offset(vaddr, len)
                .and_then(|_| range.read(vaddr, len))
        })
    }

    fn cstr(&self, vaddr: u64) -> Option<&[u8]> {
        self.ranges.iter().find_map(|range| range.cstr(vaddr))
    }
}

#[cfg(test)]
#[path = "mem_ut.rs"]
mod tests;
