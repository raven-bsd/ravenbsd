// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The arena, its boundary tags and the allocation policies.

use alloc::vec::Vec;

use crate::error::{Policy, VmemError};
use crate::segment::{Segment, SegmentState};

/// Default number of boundary tags in a new arena.
///
/// The value matches the bootstrap pool named by the kernel memory design (R-MEM-2): an empty
/// arena is one tag and each split costs at most two, so 256 tags cover the first kmem caches
/// without growing the slab. Callers with a larger expected extent count use
/// [`Vmem::with_capacity`].
pub const DEFAULT_TAG_CAPACITY: usize = 256;

/// One boundary tag: a maximal extent in one of the two states.
///
/// The arena keeps its tags sorted by address. Because every extent ends at a boundary and the
/// next tag starts there, the tag alone describes both of its physical neighbours; the sorted
/// vector lets the arena find them without intrusive pointers, which keeps the crate free of
/// unsafe code.
#[derive(Clone, Copy, Debug)]
struct Span {
    base: u64,
    size: u64,
    state: SegmentState,
}

impl Span {
    const fn new(base: u64, size: u64, state: SegmentState) -> Self {
        Self { base, size, state }
    }

    /// One past the last address. The arena invariant guarantees `base + size <= u64::MAX`.
    const fn end(self) -> u64 {
        self.base + self.size
    }

    const fn is_free(self) -> bool {
        self.state.is_free()
    }
}

/// Snapshot of the arena counters and byte accounting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VmemStats {
    /// Successful [`Vmem::alloc`] and [`Vmem::alloc_at`] calls.
    pub allocs: u64,
    /// Successful [`Vmem::free`] calls.
    pub frees: u64,
    /// Rejected allocation calls, including malformed requests and exhaustion.
    pub alloc_fail: u64,
    /// Boundary tags currently in use (free plus allocated).
    pub spans: usize,
    /// Sum of every free extent.
    pub free_bytes: u64,
    /// Sum of every allocated extent.
    pub used_bytes: u64,
    /// Size of the largest free extent.
    pub largest_free: u64,
}

/// A virtual-memory arena over a caller-supplied set of address ranges.
///
/// The arena owns a fixed-capacity tag slab allocated by [`Vmem::new`] or
/// [`Vmem::with_capacity`]. Allocation and free never allocate: a request that would need a tag
/// the slab cannot hold fails with [`VmemError::NoTags`] and leaves the arena unchanged. That is
/// what allows the arena itself to sit underneath the kernel allocator.
///
/// Invariants after every successful operation:
///
/// * tags are sorted by address and do not overlap;
/// * the union of the tags is exactly the imported address set;
/// * no two adjacent tags are both free (free neighbours are coalesced);
/// * an allocated tag covers exactly the range a caller received.
#[derive(Debug)]
pub struct Vmem {
    spans: Vec<Span>,
    tag_limit: usize,
    next_fit: u64,
    allocs: u64,
    frees: u64,
    alloc_fail: u64,
}

impl Vmem {
    /// Create an arena covering `[start, start + size)` with [`DEFAULT_TAG_CAPACITY`] tags.
    ///
    /// Any later [`Vmem::import`] adds a disjoint range to the same arena.
    ///
    /// # Errors
    ///
    /// [`VmemError::ZeroLength`] for a zero length, [`VmemError::Overflow`] when the range does
    /// not fit in the address space, and [`VmemError::Capacity`] when the tag slab cannot be
    /// allocated.
    pub fn new(start: u64, size: u64) -> Result<Self, VmemError> {
        Self::with_capacity(start, size, DEFAULT_TAG_CAPACITY)
    }

    /// Create an arena covering `[start, start + size)` with room for `max_tags` boundary tags.
    ///
    /// The tag slab is allocated here, once. A `max_tags` of zero is rejected with
    /// [`VmemError::NoTags`]: at least one tag is needed for the initial range.
    ///
    /// # Errors
    ///
    /// The same errors as [`Vmem::new`], plus [`VmemError::NoTags`] for a zero tag capacity.
    pub fn with_capacity(start: u64, size: u64, max_tags: usize) -> Result<Self, VmemError> {
        if max_tags == 0 {
            return Err(VmemError::NoTags);
        }
        let mut spans = Vec::new();
        spans
            .try_reserve_exact(max_tags)
            .map_err(|_| VmemError::Capacity)?;
        let mut arena = Self {
            spans,
            tag_limit: max_tags,
            next_fit: start,
            allocs: 0,
            frees: 0,
            alloc_fail: 0,
        };
        arena.import(start, size)?;
        Ok(arena)
    }

    /// Import an additional address range into the arena.
    ///
    /// The range becomes free space. A range that touches an existing free span is coalesced with
    /// it, so two imports that meet become one span. The range must not overlap any tracked span;
    /// unlike a virtual-memory map, an import never replaces an existing one.
    ///
    /// # Errors
    ///
    /// [`VmemError::ZeroLength`], [`VmemError::Overflow`], [`VmemError::Overlap`] and
    /// [`VmemError::NoTags`] when the slab needs a record for a non-mergeable range.
    pub fn import(&mut self, base: u64, size: u64) -> Result<(), VmemError> {
        if size == 0 {
            return Err(VmemError::ZeroLength);
        }
        let end = base.checked_add(size).ok_or(VmemError::Overflow)?;
        let idx = self.spans.partition_point(|span| span.base < base);
        if idx > 0 && self.spans[idx - 1].end() > base {
            return Err(VmemError::Overlap);
        }
        if idx < self.spans.len() && end > self.spans[idx].base {
            return Err(VmemError::Overlap);
        }

        let left_free =
            idx > 0 && self.spans[idx - 1].is_free() && self.spans[idx - 1].end() == base;
        let right_free =
            idx < self.spans.len() && self.spans[idx].is_free() && self.spans[idx].base == end;

        if left_free {
            let left = idx - 1;
            let left_base = self.spans[left].base;
            self.spans[left].size = end - left_base;
            self.coalesce(left);
        } else if right_free {
            self.spans[idx].base = base;
            self.spans[idx].size = end - base;
            self.coalesce(idx);
        } else {
            if self.spans.len() >= self.tag_limit {
                return Err(VmemError::NoTags);
            }
            self.spans
                .insert(idx, Span::new(base, size, SegmentState::Free));
            self.coalesce(idx);
        }
        Ok(())
    }

    /// Return `true` when `addr` lies inside an imported range, free or allocated.
    pub fn contains(&self, addr: u64) -> bool {
        let idx = self.spans.partition_point(|span| span.base <= addr);
        idx > 0 && addr < self.spans[idx - 1].end()
    }

    /// Return `true` when the whole range `[addr, addr + size)` is imported.
    ///
    /// A zero length is treated as a probe for `addr`, so it follows [`Vmem::contains`].
    pub fn contains_range(&self, addr: u64, size: u64) -> bool {
        if size == 0 {
            return self.contains(addr);
        }
        let Some(end) = addr.checked_add(size) else {
            return false;
        };
        let idx = self.spans.partition_point(|span| span.base <= addr);
        if idx == 0 {
            return false;
        }
        let span = self.spans[idx - 1];
        addr < span.end() && end <= span.end()
    }

    /// Allocate `size` bytes at an address aligned to the power-of-two `align`.
    ///
    /// Uses [`Policy::BestFit`]. On success the returned address belongs to a segment that remains
    /// allocated until passed back to [`Vmem::free`] with the same size.
    ///
    /// # Errors
    ///
    /// [`VmemError::ZeroLength`], [`VmemError::Alignment`], [`VmemError::Exhausted`] and
    /// [`VmemError::NoTags`]. A failed call leaves the arena unchanged.
    pub fn alloc(&mut self, size: u64, align: u64) -> Result<u64, VmemError> {
        self.alloc_with(size, align, Policy::BestFit)
    }

    /// Allocate `size` bytes with the given search [`Policy`].
    ///
    /// # Errors
    ///
    /// The same errors as [`Vmem::alloc`].
    pub fn alloc_with(&mut self, size: u64, align: u64, policy: Policy) -> Result<u64, VmemError> {
        match self.try_alloc(size, align, policy) {
            Ok(base) => {
                self.allocs = self.allocs.saturating_add(1);
                Ok(base)
            }
            Err(error) => {
                self.alloc_fail = self.alloc_fail.saturating_add(1);
                Err(error)
            }
        }
    }

    /// Allocate exactly at `addr`.
    ///
    /// The whole range must be free, `addr` must be aligned to the power-of-two `align`, and the
    /// range must not cross a boundary tag.
    ///
    /// # Errors
    ///
    /// [`VmemError::ZeroLength`], [`VmemError::Alignment`], [`VmemError::Overflow`],
    /// [`VmemError::OutOfRange`], [`VmemError::Overlap`] and [`VmemError::NoTags`]. A failed call
    /// leaves the arena unchanged.
    pub fn alloc_at(&mut self, addr: u64, size: u64, align: u64) -> Result<(), VmemError> {
        match self.try_alloc_at(addr, size, align) {
            Ok(()) => {
                self.allocs = self.allocs.saturating_add(1);
                Ok(())
            }
            Err(error) => {
                self.alloc_fail = self.alloc_fail.saturating_add(1);
                Err(error)
            }
        }
    }

    /// Return the segment starting at `base` with length `size` to the free pool.
    ///
    /// The call requires the exact address and length of an allocated segment; it never frees a
    /// sub-range. Free neighbours are coalesced immediately.
    ///
    /// # Errors
    ///
    /// [`VmemError::ZeroLength`], [`VmemError::UnknownSegment`], [`VmemError::DoubleFree`] and
    /// [`VmemError::SizeMismatch`]. A failed call leaves the arena unchanged.
    pub fn free(&mut self, base: u64, size: u64) -> Result<(), VmemError> {
        if size == 0 {
            return Err(VmemError::ZeroLength);
        }
        let idx = self.spans.partition_point(|span| span.base < base);
        if idx >= self.spans.len() || self.spans[idx].base != base {
            return Err(VmemError::UnknownSegment);
        }
        if self.spans[idx].is_free() {
            return Err(VmemError::DoubleFree);
        }
        if self.spans[idx].size != size {
            return Err(VmemError::SizeMismatch);
        }
        self.spans[idx].state = SegmentState::Free;
        self.coalesce(idx);
        self.frees = self.frees.saturating_add(1);
        Ok(())
    }

    /// Number of boundary tags in use (free plus allocated segments).
    pub fn span_count(&self) -> usize {
        self.spans.len()
    }

    /// Number of free segments.
    pub fn free_span_count(&self) -> usize {
        self.spans.iter().filter(|span| span.is_free()).count()
    }

    /// Number of allocated segments.
    pub fn allocated_span_count(&self) -> usize {
        self.spans.iter().filter(|span| !span.is_free()).count()
    }

    /// Maximum number of boundary tags the arena can hold.
    pub fn tag_capacity(&self) -> usize {
        self.tag_limit
    }

    /// Size of the largest free segment, or zero when nothing is free.
    pub fn largest_free(&self) -> u64 {
        self.spans
            .iter()
            .filter(|span| span.is_free())
            .map(|span| span.size)
            .max()
            .unwrap_or(0)
    }

    /// Sum of all free segments.
    pub fn total_free(&self) -> u64 {
        self.spans
            .iter()
            .filter(|span| span.is_free())
            .fold(0u64, |total, span| total.saturating_add(span.size))
    }

    /// Sum of all allocated segments.
    pub fn total_allocated(&self) -> u64 {
        self.spans
            .iter()
            .filter(|span| !span.is_free())
            .fold(0u64, |total, span| total.saturating_add(span.size))
    }

    /// Fragmentation of the free pool: `1 - largest_free / total_free`.
    ///
    /// Zero means one free segment covers everything free; approaching one means free memory is
    /// scattered. With no free memory at all the value is `1.0`. The ratio is exposed for
    /// statistics only; the exact integers stay available through [`Vmem::largest_free`] and
    /// [`Vmem::total_free`].
    pub fn fragmentation(&self) -> f32 {
        let total = self.total_free();
        if total == 0 {
            return 1.0;
        }
        1.0 - (self.largest_free() as f32 / total as f32)
    }

    /// Iterate over every tracked segment in address order.
    pub fn segments(&self) -> impl Iterator<Item = Segment> + '_ {
        self.spans
            .iter()
            .map(|span| Segment::new(span.base, span.size, span.state))
    }

    /// Iterate over the allocated segments in address order.
    pub fn allocated_segments(&self) -> impl Iterator<Item = Segment> + '_ {
        self.segments().filter(|segment| !segment.is_free())
    }

    /// Iterate over the free segments in address order.
    pub fn free_segments(&self) -> impl Iterator<Item = Segment> + '_ {
        self.segments().filter(|segment| segment.is_free())
    }

    /// Snapshot the counters and byte accounting.
    pub fn stats(&self) -> VmemStats {
        VmemStats {
            allocs: self.allocs,
            frees: self.frees,
            alloc_fail: self.alloc_fail,
            spans: self.spans.len(),
            free_bytes: self.total_free(),
            used_bytes: self.total_allocated(),
            largest_free: self.largest_free(),
        }
    }

    fn try_alloc(&mut self, size: u64, align: u64, policy: Policy) -> Result<u64, VmemError> {
        if size == 0 {
            return Err(VmemError::ZeroLength);
        }
        if !align.is_power_of_two() {
            return Err(VmemError::Alignment);
        }
        let (idx, base) = self.find(size, align, policy).ok_or(VmemError::Exhausted)?;
        self.carve(idx, base, size)?;
        self.next_fit = base + size;
        Ok(base)
    }

    fn try_alloc_at(&mut self, addr: u64, size: u64, align: u64) -> Result<(), VmemError> {
        if size == 0 {
            return Err(VmemError::ZeroLength);
        }
        if !align.is_power_of_two() {
            return Err(VmemError::Alignment);
        }
        if addr & (align - 1) != 0 {
            return Err(VmemError::Alignment);
        }
        let end = addr.checked_add(size).ok_or(VmemError::Overflow)?;
        let Some(idx) = self
            .spans
            .partition_point(|span| span.base <= addr)
            .checked_sub(1)
        else {
            return Err(VmemError::OutOfRange);
        };
        let span = self.spans[idx];
        if addr >= span.end() {
            return Err(VmemError::OutOfRange);
        }
        if !span.is_free() {
            return Err(VmemError::Overlap);
        }
        if end > span.end() {
            return Err(VmemError::OutOfRange);
        }
        self.carve(idx, addr, size)?;
        self.next_fit = end;
        Ok(())
    }

    /// Find a free span that can hold `size` at `align` and return its index and candidate base.
    fn find(&self, size: u64, align: u64, policy: Policy) -> Option<(usize, u64)> {
        if policy == Policy::NextFit {
            let start = self.spans.partition_point(|span| span.base < self.next_fit);
            let len = self.spans.len();
            for step in 0..len {
                let mut idx = start + step;
                if idx >= len {
                    idx -= len;
                }
                if let Some(base) = fit(self.spans[idx], size, align) {
                    return Some((idx, base));
                }
            }
            return None;
        }

        let mut best: Option<(usize, u64, u64)> = None;
        for (idx, span) in self.spans.iter().enumerate() {
            let Some(base) = fit(*span, size, align) else {
                continue;
            };
            if policy == Policy::FirstFit {
                return Some((idx, base));
            }
            let leftover = span.end() - (base + size);
            if best.is_none_or(|current| leftover < current.2) {
                best = Some((idx, base, leftover));
            }
        }
        best.map(|(idx, base, _)| (idx, base))
    }

    /// Split the free span at `idx` so that `[base, base + size)` becomes allocated.
    ///
    /// The tag-capacity check happens before the first write, so [`VmemError::NoTags`] cannot
    /// leave a half-split arena behind.
    fn carve(&mut self, idx: usize, base: u64, size: u64) -> Result<(), VmemError> {
        let span = self.spans[idx];
        let end = base + size;
        let prefix = base - span.base;
        let suffix = span.end() - end;
        let extra = if prefix > 0 {
            1 + usize::from(suffix > 0)
        } else {
            usize::from(suffix > 0)
        };
        if self.spans.len() + extra > self.tag_limit {
            return Err(VmemError::NoTags);
        }

        if prefix > 0 {
            self.spans[idx] = Span::new(span.base, prefix, SegmentState::Free);
            self.spans
                .insert(idx + 1, Span::new(base, size, SegmentState::Allocated));
            if suffix > 0 {
                self.spans
                    .insert(idx + 2, Span::new(end, suffix, SegmentState::Free));
            }
        } else {
            self.spans[idx] = Span::new(base, size, SegmentState::Allocated);
            if suffix > 0 {
                self.spans
                    .insert(idx + 1, Span::new(end, suffix, SegmentState::Free));
            }
        }
        Ok(())
    }

    /// Merge the free span at `idx` with free neighbours on either side.
    fn coalesce(&mut self, mut idx: usize) {
        if idx > 0 {
            let previous = idx - 1;
            if self.spans[previous].is_free() && self.spans[previous].end() == self.spans[idx].base
            {
                let size = self.spans[idx].size;
                self.spans[previous].size += size;
                self.spans.remove(idx);
                idx = previous;
            }
        }
        if idx + 1 < self.spans.len() {
            let next = idx + 1;
            if self.spans[next].is_free() && self.spans[idx].end() == self.spans[next].base {
                let size = self.spans[next].size;
                self.spans[idx].size += size;
                self.spans.remove(next);
            }
        }
    }
}

/// The aligned base of `span` that can hold `size`, if there is one.
fn fit(span: Span, size: u64, align: u64) -> Option<u64> {
    if !span.is_free() {
        return None;
    }
    let mask = align - 1;
    let base = span.base.checked_add(mask)? & !mask;
    let end = base.checked_add(size)?;
    (end <= span.end()).then_some(base)
}

#[cfg(test)]
#[path = "arena_ut.rs"]
mod tests;
