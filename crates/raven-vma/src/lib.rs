// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Virtual-memory-area tree.
//!
//! This crate is pure and allocation-only (`alloc`): the kernel links it to track user mappings
//! without trusting any range or length. Every operation checks page alignment, the configured
//! address range, arithmetic overflow and W^X before changing anything. Insertions are kept sorted
//! by address; removals and protection changes split partially covered areas and adjust
//! `Initfs` backing offsets so a later copy-on-write fault can still find the file page.
//!
//! The crate contains no unsafe code.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

use alloc::vec::Vec;
use core::ops::Range;

/// Page size used by every VMA range.
pub const PAGE_SIZE: u64 = 4096;

/// Where the contents of a VMA come from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backing {
    /// Process-owned anonymous frames.
    Anonymous,
    /// A private, copy-on-write mapping of initfs bytes at `offset`.
    Initfs {
        /// Byte offset of the first page inside the initfs image.
        offset: u64,
    },
}

/// Why a VMA operation was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmaError {
    /// The requested range overlaps an existing area.
    Overlap,
    /// The range is outside the tree's configured address range.
    OutOfRange,
    /// The start or length is not a multiple of [`PAGE_SIZE`].
    Misaligned,
    /// The length is zero.
    ZeroLength,
    /// The protections would allow both writes and execution.
    WriteExecute,
    /// A protection bit outside `READ | WRITE | EXEC` was supplied.
    InvalidProtection,
    /// A range that must be fully mapped contains a hole.
    Unmapped,
}

/// One contiguous virtual-memory area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Vma {
    start: u64,
    len: u64,
    prot: u8,
    backing: Backing,
}

impl Vma {
    /// First virtual address.
    pub const fn start(self) -> u64 {
        self.start
    }

    /// One past the last virtual address.
    pub const fn end(self) -> u64 {
        self.start + self.len
    }

    /// Length in bytes (a positive multiple of [`PAGE_SIZE`]).
    pub const fn len(self) -> u64 {
        self.len
    }

    /// VMAs are never empty; present for API symmetry with containers.
    pub const fn is_empty(self) -> bool {
        false
    }

    /// RavenBSD protection bits.
    pub const fn prot(self) -> u8 {
        self.prot
    }

    /// Where the contents come from.
    pub const fn backing(self) -> Backing {
        self.backing
    }

    /// True when `address` lies in this area.
    pub const fn contains(self, address: u64) -> bool {
        address >= self.start && address < self.end()
    }
}

/// A sorted collection of non-overlapping VMAs inside one address range.
#[derive(Clone, Debug)]
pub struct Vmas {
    range: Range<u64>,
    entries: Vec<Vma>,
}

impl Vmas {
    /// Create an empty tree confined to `range`.
    ///
    /// The range is not validated here; every insert is checked against it. The kernel passes
    /// `USER_MIN..USER_END`.
    pub fn new(range: Range<u64>) -> Self {
        Self {
            range,
            entries: Vec::new(),
        }
    }

    /// The configured address range.
    pub fn range(&self) -> Range<u64> {
        self.range.clone()
    }

    /// Insert a new area. Overlapping any existing area is [`VmaError::Overlap`].
    pub fn insert(
        &mut self,
        start: u64,
        len: u64,
        prot: u8,
        backing: Backing,
    ) -> Result<(), VmaError> {
        let end = self.check_span(start, len)?;
        check_prot(prot)?;
        if self
            .entries
            .iter()
            .any(|vma| vma.start < end && start < vma.end())
        {
            return Err(VmaError::Overlap);
        }
        let index = self
            .entries
            .iter()
            .position(|vma| vma.start > start)
            .unwrap_or(self.entries.len());
        self.entries.insert(
            index,
            Vma {
                start,
                len,
                prot,
                backing,
            },
        );
        Ok(())
    }

    /// Remove the intersection of `start..start + len` with the tree, splitting areas on both
    /// sides. Removing a range with no mapped pages is a no-op.
    pub fn remove(&mut self, start: u64, len: u64) -> Result<(), VmaError> {
        if len == 0 {
            return Ok(());
        }
        let end = self.check_span(start, len)?;
        let mut next = Vec::with_capacity(self.entries.len().saturating_add(2));
        for vma in &self.entries {
            if vma.end() <= start || vma.start >= end {
                next.push(*vma);
                continue;
            }
            if vma.start < start {
                next.push(Vma {
                    len: start - vma.start,
                    ..*vma
                });
            }
            if vma.end() > end {
                next.push(Vma {
                    start: end,
                    len: vma.end() - end,
                    backing: shift_backing(vma.backing, end - vma.start),
                    ..*vma
                });
            }
        }
        self.entries = next;
        Ok(())
    }

    /// Change the protection of the intersection of `start..start + len`, splitting areas as
    /// needed.
    pub fn protect(&mut self, start: u64, len: u64, prot: u8) -> Result<(), VmaError> {
        if len == 0 {
            return Ok(());
        }
        check_prot(prot)?;
        let end = self.check_span(start, len)?;
        let mut next = Vec::with_capacity(self.entries.len().saturating_add(2));
        for vma in &self.entries {
            if vma.end() <= start || vma.start >= end {
                next.push(*vma);
                continue;
            }
            let overlap_start = vma.start.max(start);
            let overlap_end = vma.end().min(end);
            if vma.start < overlap_start {
                next.push(Vma {
                    len: overlap_start - vma.start,
                    ..*vma
                });
            }
            next.push(Vma {
                start: overlap_start,
                len: overlap_end - overlap_start,
                prot,
                backing: shift_backing(vma.backing, overlap_start - vma.start),
            });
            if vma.end() > overlap_end {
                next.push(Vma {
                    start: overlap_end,
                    len: vma.end() - overlap_end,
                    backing: shift_backing(vma.backing, overlap_end - vma.start),
                    ..*vma
                });
            }
        }
        self.entries = next;
        Ok(())
    }

    /// The area containing `address`, if any.
    pub fn find(&self, address: u64) -> Option<Vma> {
        self.entries
            .iter()
            .copied()
            .find(|vma| vma.contains(address))
    }

    /// First page-aligned free range of `len` bytes at or above `hint`, aligned to `align`.
    ///
    /// `align` is raised to at least [`PAGE_SIZE`] and must be a power of two.
    pub fn find_free(&self, len: u64, hint: u64, align: u64) -> Option<u64> {
        if len == 0 || !len.is_multiple_of(PAGE_SIZE) {
            return None;
        }
        if align != 0 && !align.is_power_of_two() {
            return None;
        }
        let align = align.max(PAGE_SIZE);
        let mut candidate = align_up(hint.max(self.range.start), align)?;
        if candidate < self.range.start {
            candidate = align_up(self.range.start, align)?;
        }
        loop {
            let end = candidate.checked_add(len)?;
            if end > self.range.end {
                return None;
            }
            let mut next = None;
            for vma in &self.entries {
                if vma.end() <= candidate {
                    continue;
                }
                if vma.start >= end {
                    break;
                }
                next = Some(align_up(vma.end(), align)?);
                break;
            }
            match next {
                None => return Some(candidate),
                Some(n) if n > candidate => candidate = n,
                Some(_) => return None,
            }
        }
    }

    /// True when `start..start + len` is inside the tree's range and overlaps nothing.
    pub fn is_free(&self, start: u64, len: u64) -> bool {
        let Ok(end) = self.check_span(start, len) else {
            return false;
        };
        !self
            .entries
            .iter()
            .any(|vma| vma.start < end && start < vma.end())
    }

    /// True when every page of `start..start + len` is covered by a VMA.
    pub fn is_fully_mapped(&self, start: u64, len: u64) -> bool {
        if len == 0 {
            return false;
        }
        let Ok(end) = self.check_span(start, len) else {
            return false;
        };
        let mut cursor = start;
        for vma in &self.entries {
            if vma.end() <= cursor {
                continue;
            }
            if vma.start > cursor {
                return false;
            }
            cursor = vma.end();
            if cursor >= end {
                return true;
            }
        }
        false
    }

    /// Number of areas.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when there are no areas.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Areas in ascending address order.
    pub fn iter(&self) -> impl Iterator<Item = &Vma> {
        self.entries.iter()
    }

    fn check_span(&self, start: u64, len: u64) -> Result<u64, VmaError> {
        if len == 0 {
            return Err(VmaError::ZeroLength);
        }
        if !start.is_multiple_of(PAGE_SIZE) || !len.is_multiple_of(PAGE_SIZE) {
            return Err(VmaError::Misaligned);
        }
        let end = start.checked_add(len).ok_or(VmaError::OutOfRange)?;
        if start < self.range.start || end > self.range.end {
            return Err(VmaError::OutOfRange);
        }
        Ok(end)
    }
}

fn check_prot(prot: u8) -> Result<(), VmaError> {
    if prot & !0b111 != 0 {
        return Err(VmaError::InvalidProtection);
    }
    if prot & 0b010 != 0 && prot & 0b100 != 0 {
        return Err(VmaError::WriteExecute);
    }
    Ok(())
}

fn shift_backing(backing: Backing, delta: u64) -> Backing {
    match backing {
        Backing::Anonymous => Backing::Anonymous,
        Backing::Initfs { offset } => Backing::Initfs {
            offset: offset.saturating_add(delta),
        },
    }
}

fn align_up(value: u64, align: u64) -> Option<u64> {
    value
        .checked_add(align.checked_sub(1)?)
        .map(|rounded| rounded & !(align - 1))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    const LOW: u64 = 0x1000;
    const HIGH: u64 = 0x1000_0000;

    fn tree() -> Vmas {
        Vmas::new(LOW..HIGH)
    }

    fn insert(tree: &mut Vmas, page: u64, pages: u64, prot: u8) {
        tree.insert(
            LOW + page * PAGE_SIZE,
            pages * PAGE_SIZE,
            prot,
            Backing::Anonymous,
        )
        .unwrap();
    }

    #[test]
    fn insert_and_find_one_area() {
        let mut tree = tree();
        insert(&mut tree, 1, 2, 0b011);
        let vma = tree.find(LOW + PAGE_SIZE).unwrap();
        assert_eq!(vma.start(), LOW + PAGE_SIZE);
        assert_eq!(vma.end(), LOW + 3 * PAGE_SIZE);
        assert_eq!(vma.prot(), 0b011);
        assert_eq!(vma.backing(), Backing::Anonymous);
        assert!(tree.find(LOW).is_none());
    }

    #[test]
    fn entries_are_sorted_by_address() {
        let mut tree = tree();
        insert(&mut tree, 5, 1, 0);
        insert(&mut tree, 1, 1, 0);
        insert(&mut tree, 3, 1, 0);
        let starts: vec::Vec<u64> = tree.iter().map(|vma| vma.start()).collect();
        assert_eq!(
            starts,
            vec![LOW + PAGE_SIZE, LOW + 3 * PAGE_SIZE, LOW + 5 * PAGE_SIZE]
        );
    }

    #[test]
    fn overlapping_insert_is_rejected_in_all_shapes() {
        let mut tree = tree();
        insert(&mut tree, 4, 4, 0);
        for (page, pages) in [(3, 2), (7, 2), (5, 1), (3, 10), (4, 4)] {
            let start = LOW + page * PAGE_SIZE;
            assert_eq!(
                tree.insert(start, pages * PAGE_SIZE, 0, Backing::Anonymous),
                Err(VmaError::Overlap),
                "page {page} len {pages}"
            );
        }
    }

    #[test]
    fn adjacent_inserts_are_allowed() {
        let mut tree = tree();
        insert(&mut tree, 1, 1, 0);
        insert(&mut tree, 2, 1, 0);
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn writable_and_executable_is_rejected() {
        let mut tree = tree();
        assert_eq!(
            tree.insert(LOW, PAGE_SIZE, 0b110, Backing::Anonymous),
            Err(VmaError::WriteExecute)
        );
    }

    #[test]
    fn unknown_protection_bits_are_rejected() {
        let mut tree = tree();
        assert_eq!(
            tree.insert(LOW, PAGE_SIZE, 0b1000, Backing::Anonymous),
            Err(VmaError::InvalidProtection)
        );
    }

    #[test]
    fn misaligned_and_zero_length_are_rejected() {
        let mut tree = tree();
        assert_eq!(
            tree.insert(LOW + 1, PAGE_SIZE, 0, Backing::Anonymous),
            Err(VmaError::Misaligned)
        );
        assert_eq!(
            tree.insert(LOW, PAGE_SIZE + 1, 0, Backing::Anonymous),
            Err(VmaError::Misaligned)
        );
        assert_eq!(
            tree.insert(LOW, 0, 0, Backing::Anonymous),
            Err(VmaError::ZeroLength)
        );
    }

    #[test]
    fn out_of_range_are_rejected() {
        let mut tree = tree();
        assert_eq!(
            tree.insert(LOW - PAGE_SIZE, PAGE_SIZE, 0, Backing::Anonymous),
            Err(VmaError::OutOfRange)
        );
        assert_eq!(
            tree.insert(HIGH, PAGE_SIZE, 0, Backing::Anonymous),
            Err(VmaError::OutOfRange)
        );
        assert_eq!(
            tree.insert(HIGH - PAGE_SIZE, 2 * PAGE_SIZE, 0, Backing::Anonymous),
            Err(VmaError::OutOfRange)
        );
    }

    #[test]
    fn end_overflow_is_rejected() {
        let mut tree = tree();
        assert_eq!(
            tree.insert(u64::MAX - PAGE_SIZE + 1, PAGE_SIZE, 0, Backing::Anonymous),
            Err(VmaError::OutOfRange)
        );
    }

    #[test]
    fn remove_entire_area() {
        let mut tree = tree();
        insert(&mut tree, 1, 4, 0);
        tree.remove(LOW + PAGE_SIZE, 4 * PAGE_SIZE).unwrap();
        assert!(tree.is_empty());
    }

    #[test]
    fn remove_left_part() {
        let mut tree = tree();
        insert(&mut tree, 1, 4, 0);
        tree.remove(LOW + PAGE_SIZE, 2 * PAGE_SIZE).unwrap();
        assert_eq!(tree.len(), 1);
        assert_eq!(
            tree.find(LOW + 3 * PAGE_SIZE).unwrap().start(),
            LOW + 3 * PAGE_SIZE
        );
    }

    #[test]
    fn remove_right_part() {
        let mut tree = tree();
        insert(&mut tree, 1, 4, 0);
        tree.remove(LOW + 3 * PAGE_SIZE, 2 * PAGE_SIZE).unwrap();
        let vma = tree.find(LOW + PAGE_SIZE).unwrap();
        assert_eq!(vma.end(), LOW + 3 * PAGE_SIZE);
    }

    #[test]
    fn remove_middle_splits_area() {
        let mut tree = tree();
        insert(&mut tree, 1, 6, 0);
        tree.remove(LOW + 3 * PAGE_SIZE, 2 * PAGE_SIZE).unwrap();
        assert_eq!(tree.len(), 2);
        assert_eq!(
            tree.find(LOW + PAGE_SIZE).unwrap().end(),
            LOW + 3 * PAGE_SIZE
        );
        assert_eq!(
            tree.find(LOW + 5 * PAGE_SIZE).unwrap().start(),
            LOW + 5 * PAGE_SIZE
        );
        assert!(tree.find(LOW + 3 * PAGE_SIZE).is_none());
    }

    #[test]
    fn remove_splits_initfs_offset() {
        let mut tree = tree();
        let start = LOW + PAGE_SIZE;
        tree.insert(
            start,
            6 * PAGE_SIZE,
            0,
            Backing::Initfs { offset: 0x10_0000 },
        )
        .unwrap();
        tree.remove(LOW + 3 * PAGE_SIZE, 2 * PAGE_SIZE).unwrap();
        let right = tree.find(LOW + 5 * PAGE_SIZE).unwrap();
        assert_eq!(
            right.backing(),
            Backing::Initfs {
                offset: 0x10_0000 + 4 * PAGE_SIZE
            }
        );
        let left = tree.find(LOW + PAGE_SIZE).unwrap();
        assert_eq!(left.backing(), Backing::Initfs { offset: 0x10_0000 });
    }

    #[test]
    fn remove_no_overlap_is_noop() {
        let mut tree = tree();
        insert(&mut tree, 1, 2, 0);
        tree.remove(LOW + 10 * PAGE_SIZE, PAGE_SIZE).unwrap();
        assert_eq!(tree.len(), 1);
    }

    #[test]
    fn remove_across_two_areas() {
        let mut tree = tree();
        insert(&mut tree, 1, 2, 0);
        insert(&mut tree, 3, 2, 0);
        tree.remove(LOW + 2 * PAGE_SIZE, 2 * PAGE_SIZE).unwrap();
        assert_eq!(tree.len(), 2);
        assert_eq!(
            tree.find(LOW + PAGE_SIZE).unwrap().end(),
            LOW + 2 * PAGE_SIZE
        );
        assert_eq!(
            tree.find(LOW + 4 * PAGE_SIZE).unwrap().start(),
            LOW + 4 * PAGE_SIZE
        );
    }

    #[test]
    fn protect_middle_splits_and_changes_only_middle() {
        let mut tree = tree();
        insert(&mut tree, 1, 6, 0b001);
        tree.protect(LOW + 3 * PAGE_SIZE, 2 * PAGE_SIZE, 0b011)
            .unwrap();
        assert_eq!(tree.len(), 3);
        assert_eq!(tree.find(LOW + PAGE_SIZE).unwrap().prot(), 0b001);
        assert_eq!(tree.find(LOW + 3 * PAGE_SIZE).unwrap().prot(), 0b011);
        assert_eq!(tree.find(LOW + 5 * PAGE_SIZE).unwrap().prot(), 0b001);
    }

    #[test]
    fn protect_left_and_right_parts() {
        let mut tree = tree();
        insert(&mut tree, 1, 4, 0);
        tree.protect(LOW + PAGE_SIZE, PAGE_SIZE, 0b001).unwrap();
        tree.protect(LOW + 4 * PAGE_SIZE, PAGE_SIZE, 0b001).unwrap();
        assert_eq!(tree.find(LOW + 2 * PAGE_SIZE).unwrap().prot(), 0);
        assert_eq!(tree.find(LOW + 4 * PAGE_SIZE).unwrap().prot(), 0b001);
    }

    #[test]
    fn protect_splits_initfs_offset() {
        let mut tree = tree();
        tree.insert(
            LOW + PAGE_SIZE,
            6 * PAGE_SIZE,
            0,
            Backing::Initfs { offset: 0x20_0000 },
        )
        .unwrap();
        tree.protect(LOW + 3 * PAGE_SIZE, 2 * PAGE_SIZE, 0b011)
            .unwrap();
        assert_eq!(
            tree.find(LOW + 3 * PAGE_SIZE).unwrap().backing(),
            Backing::Initfs {
                offset: 0x20_0000 + 2 * PAGE_SIZE
            }
        );
        assert_eq!(
            tree.find(LOW + 5 * PAGE_SIZE).unwrap().backing(),
            Backing::Initfs {
                offset: 0x20_0000 + 4 * PAGE_SIZE
            }
        );
    }

    #[test]
    fn protect_writable_execute_is_rejected() {
        let mut tree = tree();
        insert(&mut tree, 1, 2, 0);
        assert_eq!(
            tree.protect(LOW + PAGE_SIZE, PAGE_SIZE, 0b110),
            Err(VmaError::WriteExecute)
        );
    }

    #[test]
    fn protect_with_no_overlap_is_noop() {
        let mut tree = tree();
        insert(&mut tree, 1, 2, 0);
        tree.protect(LOW + 10 * PAGE_SIZE, PAGE_SIZE, 0b011)
            .unwrap();
        assert_eq!(tree.find(LOW + PAGE_SIZE).unwrap().prot(), 0);
    }

    #[test]
    fn find_free_in_empty_tree_uses_range_start() {
        let tree = tree();
        assert_eq!(tree.find_free(PAGE_SIZE, 0, PAGE_SIZE), Some(LOW));
    }

    #[test]
    fn find_free_above_hint() {
        let tree = tree();
        assert_eq!(
            tree.find_free(PAGE_SIZE, LOW + 8 * PAGE_SIZE, PAGE_SIZE),
            Some(LOW + 8 * PAGE_SIZE)
        );
    }

    #[test]
    fn find_free_skips_existing_area() {
        let mut tree = tree();
        insert(&mut tree, 1, 3, 0);
        assert_eq!(tree.find_free(PAGE_SIZE, 0, PAGE_SIZE), Some(LOW));
        assert_eq!(
            tree.find_free(PAGE_SIZE, LOW + 4 * PAGE_SIZE, PAGE_SIZE),
            Some(LOW + 4 * PAGE_SIZE)
        );
    }

    #[test]
    fn find_free_uses_gap_after_hint() {
        let mut tree = tree();
        insert(&mut tree, 1, 2, 0);
        insert(&mut tree, 5, 2, 0);
        assert_eq!(
            tree.find_free(2 * PAGE_SIZE, LOW + PAGE_SIZE, PAGE_SIZE),
            Some(LOW + 3 * PAGE_SIZE)
        );
    }

    #[test]
    fn find_free_respects_alignment() {
        let tree = tree();
        assert_eq!(
            tree.find_free(PAGE_SIZE, LOW + PAGE_SIZE, 2 * 0x20_0000),
            Some(0x40_0000)
        );
    }

    #[test]
    fn find_free_returns_none_when_no_space() {
        let mut tree = Vmas::new(LOW..LOW + 4 * PAGE_SIZE);
        insert(&mut tree, 0, 4, 0);
        assert_eq!(tree.find_free(PAGE_SIZE, 0, PAGE_SIZE), None);
    }

    #[test]
    fn find_free_rejects_invalid_length_or_alignment() {
        let tree = tree();
        assert_eq!(tree.find_free(0, 0, PAGE_SIZE), None);
        assert_eq!(tree.find_free(PAGE_SIZE + 1, 0, PAGE_SIZE), None);
        assert_eq!(tree.find_free(PAGE_SIZE, 0, 3), None);
    }

    #[test]
    fn is_free_and_fully_mapped_detect_holes() {
        let mut tree = tree();
        insert(&mut tree, 1, 2, 0);
        insert(&mut tree, 4, 2, 0);
        assert!(tree.is_free(LOW + 3 * PAGE_SIZE, PAGE_SIZE));
        assert!(!tree.is_free(LOW + 2 * PAGE_SIZE, 2 * PAGE_SIZE));
        assert!(tree.is_fully_mapped(LOW + PAGE_SIZE, 2 * PAGE_SIZE));
        assert!(!tree.is_fully_mapped(LOW + PAGE_SIZE, 4 * PAGE_SIZE));
        assert!(!tree.is_fully_mapped(LOW, PAGE_SIZE));
        assert!(!tree.is_fully_mapped(LOW, 0));
    }

    #[test]
    fn iterator_and_len_track_entries() {
        let mut tree = tree();
        assert!(tree.is_empty());
        insert(&mut tree, 1, 1, 0);
        insert(&mut tree, 3, 1, 0);
        assert_eq!(tree.len(), 2);
        let len: usize = tree.iter().count();
        assert_eq!(len, 2);
    }

    #[test]
    fn vma_contains_boundaries() {
        let mut tree = tree();
        insert(&mut tree, 1, 2, 0);
        let vma = tree.find(LOW + PAGE_SIZE).unwrap();
        assert!(vma.contains(LOW + PAGE_SIZE));
        assert!(vma.contains(LOW + 2 * PAGE_SIZE));
        assert!(!vma.contains(LOW + 3 * PAGE_SIZE));
        assert!(!vma.contains(LOW));
    }

    #[test]
    fn range_is_exposed() {
        let tree = tree();
        assert_eq!(tree.range(), LOW..HIGH);
    }

    #[test]
    fn random_operations_against_page_bitmap_model() {
        let base = 0x10_0000u64;
        let pages = 64u64;
        let mut tree = Vmas::new(base..base + pages * PAGE_SIZE);
        let mut model: vec::Vec<Option<(u8, Backing)>> = vec![None; pages as usize];
        let mut state = 0x1234_5678u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state
        };
        for _ in 0..3000 {
            let value = next();
            let op = value % 3;
            let start_page = u64::from(next() % pages as u32);
            let max_len = (pages - start_page).max(1);
            let length = 1 + u64::from(next() % max_len.min(4) as u32);
            let start = base + start_page * PAGE_SIZE;
            let len = length * PAGE_SIZE;
            let prot = [0u8, 0b001, 0b010, 0b100, 0b011, 0b101][(next() % 6) as usize];
            let backing = if next() & 1 == 0 {
                Backing::Anonymous
            } else {
                Backing::Initfs {
                    offset: u64::from(next() % 0x10_0000),
                }
            };
            let backing = match backing {
                Backing::Initfs { offset } => Backing::Initfs {
                    offset: offset & !(PAGE_SIZE - 1),
                },
                Backing::Anonymous => Backing::Anonymous,
            };
            match op {
                0 => {
                    let overlaps = (0..length).any(|i| {
                        model
                            .get((start_page + i) as usize)
                            .is_some_and(Option::is_some)
                    });
                    let result = tree.insert(start, len, prot, backing);
                    if overlaps || prot & 0b110 == 0b110 {
                        assert!(result.is_err(), "insert should fail");
                    } else {
                        assert_eq!(result, Ok(()));
                        for i in 0..length {
                            let page_backing = match backing {
                                Backing::Anonymous => Backing::Anonymous,
                                Backing::Initfs { offset } => Backing::Initfs {
                                    offset: offset + i * PAGE_SIZE,
                                },
                            };
                            if let Some(slot) = model.get_mut((start_page + i) as usize) {
                                *slot = Some((prot, page_backing));
                            }
                        }
                    }
                }
                1 => {
                    let result = tree.remove(start, len);
                    assert_eq!(result, Ok(()));
                    for i in 0..length {
                        if let Some(slot) = model.get_mut((start_page + i) as usize) {
                            *slot = None;
                        }
                    }
                }
                _ => {
                    let result = tree.protect(start, len, prot);
                    if prot & 0b110 == 0b110 {
                        assert_eq!(result, Err(VmaError::WriteExecute));
                    } else {
                        assert_eq!(result, Ok(()));
                        for i in 0..length {
                            if let Some(slot) = model.get_mut((start_page + i) as usize)
                                && let Some((_, backing)) = *slot
                            {
                                *slot = Some((prot, backing));
                            }
                        }
                    }
                }
            }

            // Every mapped page must have the model protection and backing.
            for (index, expected) in model.iter().enumerate() {
                let address = base + index as u64 * PAGE_SIZE;
                match expected {
                    Some((prot, backing)) => {
                        let vma = tree.find(address).expect("mapped page without VMA");
                        assert_eq!(vma.prot(), *prot);
                        let expected_backing = match backing {
                            Backing::Anonymous => Backing::Anonymous,
                            Backing::Initfs { offset } => Backing::Initfs {
                                offset: offset
                                    .saturating_sub((address - vma.start()) & !(PAGE_SIZE - 1)),
                            },
                        };
                        assert_eq!(vma.backing(), expected_backing);
                    }
                    None => assert!(tree.find(address).is_none()),
                }
            }
        }
    }
}
