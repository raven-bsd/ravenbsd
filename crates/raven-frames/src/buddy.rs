// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Power-of-two buddy frame allocator with zones and pager hooks.
//!
//! This is the pure, host-tested core that the kernel's physical-frame path will sit on (A5b). It
//! manages page-frame numbers only — never dereferencing physical memory — over a caller-supplied
//! power-of-two frame range. All metadata is fixed inline storage, so construction and every
//! operation are free of global allocation.

#![forbid(unsafe_code)]

/// Why a buddy operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuddyError {
    /// The requested order exceeds the managed range.
    OrderTooLarge,
    /// No block of the requested order (or a larger one) is free.
    OutOfFrames,
    /// A frame or range is outside the managed range.
    OutOfRange,
    /// The block is already free.
    DoubleFree,
    /// The range overlaps an allocated or reserved frame.
    Occupied,
    /// A frame is not aligned to its order, or a range is not buddy-aligned.
    Misaligned,
    /// The pager refused to back a freshly allocated range.
    Pager,
    /// A zero-size request.
    ZeroSize,
}

/// Hooks the allocator calls to back (map) and unback (unmap) frame ranges.
pub trait Pager {
    /// Back `count` frames starting at `frame`; return `false` to fail the allocation.
    fn back(&mut self, frame: u64, count: u64) -> bool;
    /// Unback `count` frames starting at `frame` after they are freed.
    fn unback(&mut self, frame: u64, count: u64);
}

/// A pager that always succeeds (the default for host models).
#[derive(Clone, Copy, Debug, Default)]
pub struct NoPager;

impl Pager for NoPager {
    fn back(&mut self, _frame: u64, _count: u64) -> bool {
        true
    }

    fn unback(&mut self, _frame: u64, _count: u64) {}
}

/// Zone accounting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BuddyStats {
    /// Managed frames.
    pub total: u64,
    /// Free frames.
    pub free: u64,
    /// Reserved frames.
    pub reserved: u64,
    /// Successful allocations.
    pub allocs: u64,
    /// Successful frees.
    pub frees: u64,
    /// Rejected allocations.
    pub alloc_fail: u64,
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BlockState {
    Free,
    Allocated,
    Reserved,
}

/// The state of one frame as observed from the outside (A5c kernel integration).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameState {
    /// The frame belongs to a free block.
    Free,
    /// The frame belongs to an allocated block.
    Allocated,
    /// The frame was reserved and is never handed out until released.
    Reserved,
}

/// Sentinel for "no next block" in a free list.
const NONE: u32 = u32::MAX;

/// One order-0 block's descriptor: its state, the order of the buddy block it belongs to, and the
/// free-list link for the head of a free block.
///
/// The type is public only so a caller (the kernel) can declare a fixed storage array in BSS; its
/// fields and constructors are the allocator's business.
#[derive(Clone, Copy, Debug)]
pub struct Block {
    state: BlockState,
    order: u8,
    next_free: u32,
}

impl Block {
    /// A free descriptor with no free-list link.
    pub const fn empty() -> Self {
        Self {
            state: BlockState::Free,
            order: 0,
            next_free: NONE,
        }
    }

    /// An all-zero descriptor, so a caller's storage array lands in `.bss`; the allocator resets
    /// every descriptor during construction, so the initial value only affects placement.
    pub const fn zeroed() -> Self {
        Self {
            state: BlockState::Free,
            order: 0,
            next_free: 0,
        }
    }
}

/// A power-of-two buddy frame allocator over a caller-supplied frame range.
///
/// `ORDERS` is the number of buddy orders (one more than the maximum order). The caller provides
/// the order-0 descriptor storage (`blocks`), so the allocator itself is small and the kernel can
/// keep the ~1 MiB array in BSS. The pager `P` is consulted on every allocation and free.
pub struct BuddyAllocator<'a, const ORDERS: usize, P: Pager = NoPager> {
    blocks: &'a mut [Block],
    free_heads: [u32; ORDERS],
    base_frame: u64,
    frame_count: u64,
    top_order: usize,
    pager: P,
    reserved: u64,
    allocs: u64,
    frees: u64,
    alloc_fail: u64,
}

impl<'a, const ORDERS: usize, P: Pager + Default> BuddyAllocator<'a, ORDERS, P> {
    /// Create an allocator over `[base_frame, base_frame + frame_count)` with the default pager.
    pub fn new(
        blocks: &'a mut [Block],
        base_frame: u64,
        frame_count: u64,
    ) -> Result<Self, BuddyError> {
        Self::with_pager(blocks, base_frame, frame_count, P::default())
    }
}

impl<'a, const ORDERS: usize, P: Pager> BuddyAllocator<'a, ORDERS, P> {
    /// Create an allocator over `[base_frame, base_frame + frame_count)` with an explicit pager.
    ///
    /// `frame_count` must be a non-zero power of two no greater than `blocks.len()`, and its
    /// base-2 logarithm must be less than `ORDERS`.
    pub fn with_pager(
        blocks: &'a mut [Block],
        base_frame: u64,
        frame_count: u64,
        pager: P,
    ) -> Result<Self, BuddyError> {
        if frame_count == 0
            || !frame_count.is_power_of_two()
            || (frame_count as usize) > blocks.len()
        {
            return Err(BuddyError::OutOfRange);
        }
        let top_order = frame_count.trailing_zeros() as usize;
        if top_order >= ORDERS {
            return Err(BuddyError::OrderTooLarge);
        }
        for block in blocks.iter_mut() {
            *block = Block::empty();
        }
        let mut allocator = Self {
            blocks,
            free_heads: [NONE; ORDERS],
            base_frame,
            frame_count,
            top_order,
            pager,
            reserved: 0,
            allocs: 0,
            frees: 0,
            alloc_fail: 0,
        };
        // The whole range starts as one free block at the top order.
        allocator.mark_block(0, top_order, BlockState::Free);
        allocator.push_free(top_order, 0);
        Ok(allocator)
    }

    /// First frame in the managed range.
    pub const fn base_frame(&self) -> u64 {
        self.base_frame
    }

    /// Number of managed frames.
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Return `true` when `frame` is in the managed range.
    pub fn contains(&self, frame: u64) -> bool {
        frame >= self.base_frame && frame < self.base_frame.saturating_add(self.frame_count)
    }

    /// The state of `frame`, or `None` when it is outside the managed range.
    pub fn state(&self, frame: u64) -> Option<FrameState> {
        let index = usize::try_from(frame.checked_sub(self.base_frame)?).ok()?;
        if index >= self.frame_count as usize {
            return None;
        }
        Some(match self.blocks[index].state {
            BlockState::Free => FrameState::Free,
            BlockState::Allocated => FrameState::Allocated,
            BlockState::Reserved => FrameState::Reserved,
        })
    }

    /// Allocate a block of `2^order` contiguous frames.
    ///
    /// # Errors
    ///
    /// [`BuddyError::OrderTooLarge`], [`BuddyError::OutOfFrames`] and [`BuddyError::Pager`].
    pub fn alloc(&mut self, order: usize) -> Result<u64, BuddyError> {
        if order > self.top_order {
            return Err(BuddyError::OrderTooLarge);
        }
        let mut current = order;
        while current <= self.top_order && self.free_heads[current] == NONE {
            current += 1;
        }
        if current > self.top_order {
            self.alloc_fail = self.alloc_fail.saturating_add(1);
            return Err(BuddyError::OutOfFrames);
        }
        let index = self.pop_free(current);
        while current > order {
            let buddy = index + (1 << (current - 1));
            self.mark_block(buddy, current - 1, BlockState::Free);
            self.push_free(current - 1, buddy);
            current -= 1;
        }
        self.mark_block(index, order, BlockState::Allocated);
        let frame = self.base_frame + index as u64;
        if !self.pager.back(frame, 1u64 << order) {
            self.mark_block(index, order, BlockState::Free);
            self.push_free(order, index);
            self.alloc_fail = self.alloc_fail.saturating_add(1);
            return Err(BuddyError::Pager);
        }
        self.allocs = self.allocs.saturating_add(1);
        Ok(frame)
    }

    /// Allocate the block of `2^order` frames starting exactly at `frame`.
    ///
    /// # Errors
    ///
    /// [`BuddyError::OrderTooLarge`], [`BuddyError::OutOfRange`], [`BuddyError::Misaligned`],
    /// [`BuddyError::Occupied`] and [`BuddyError::Pager`].
    pub fn alloc_at(&mut self, frame: u64, order: usize) -> Result<(), BuddyError> {
        if order > self.top_order {
            return Err(BuddyError::OrderTooLarge);
        }
        let Some(index) = frame.checked_sub(self.base_frame) else {
            return Err(BuddyError::OutOfRange);
        };
        let index = index as usize;
        if index + (1 << order) > self.frame_count as usize {
            return Err(BuddyError::OutOfRange);
        }
        if !index.is_multiple_of(1 << order) {
            return Err(BuddyError::Misaligned);
        }
        if self.blocks[index].state != BlockState::Free {
            return Err(BuddyError::Occupied);
        }
        let Some(head) = self.carve(index, order) else {
            return Err(BuddyError::Occupied);
        };
        debug_assert_eq!(head, index);
        self.mark_block(index, order, BlockState::Allocated);
        let frame = self.base_frame + index as u64;
        if !self.pager.back(frame, 1u64 << order) {
            self.mark_block(index, order, BlockState::Free);
            self.push_free(order, index);
            self.alloc_fail = self.alloc_fail.saturating_add(1);
            return Err(BuddyError::Pager);
        }
        self.allocs = self.allocs.saturating_add(1);
        Ok(())
    }

    /// Free the block of `2^order` frames starting at `frame`.
    ///
    /// # Errors
    ///
    /// [`BuddyError::OrderTooLarge`], [`BuddyError::OutOfRange`], [`BuddyError::Misaligned`] and
    /// [`BuddyError::DoubleFree`].
    pub fn free(&mut self, frame: u64, order: usize) -> Result<(), BuddyError> {
        if order > self.top_order {
            return Err(BuddyError::OrderTooLarge);
        }
        let Some(index) = frame.checked_sub(self.base_frame) else {
            return Err(BuddyError::OutOfRange);
        };
        let index = index as usize;
        if index + (1 << order) > self.frame_count as usize {
            return Err(BuddyError::OutOfRange);
        }
        if !index.is_multiple_of(1 << order) {
            return Err(BuddyError::Misaligned);
        }
        if self.blocks[index].state == BlockState::Free {
            return Err(BuddyError::DoubleFree);
        }
        if self.blocks[index].state != BlockState::Allocated
            || self.blocks[index].order != order as u8
        {
            return Err(BuddyError::OutOfRange);
        }
        self.pager.unback(frame, 1u64 << order);
        self.mark_block(index, order, BlockState::Free);
        self.coalesce(index, order);
        self.frees = self.frees.saturating_add(1);
        Ok(())
    }

    /// Release one order-0 frame that is either allocated or reserved (A5c loader reclaim).
    ///
    /// A reserved frame returns to the free lists and its reservation count drops; an allocated
    /// order-0 frame behaves like [`free`](Self::free). A frame that belongs to a larger block is
    /// rejected with [`BuddyError::OutOfRange`] so a partial release cannot corrupt the block.
    ///
    /// # Errors
    ///
    /// [`BuddyError::OutOfRange`] and [`BuddyError::DoubleFree`].
    pub fn release(&mut self, frame: u64) -> Result<(), BuddyError> {
        let Some(index) = frame.checked_sub(self.base_frame) else {
            return Err(BuddyError::OutOfRange);
        };
        let index = usize::try_from(index).map_err(|_| BuddyError::OutOfRange)?;
        if index >= self.frame_count as usize {
            return Err(BuddyError::OutOfRange);
        }
        let state = self.blocks[index].state;
        if state == BlockState::Free {
            return Err(BuddyError::DoubleFree);
        }
        if self.blocks[index].order != 0 {
            return Err(BuddyError::OutOfRange);
        }
        self.mark_block(index, 0, BlockState::Free);
        self.coalesce(index, 0);
        if state == BlockState::Reserved {
            self.reserved = self.reserved.saturating_sub(1);
        }
        self.frees = self.frees.saturating_add(1);
        Ok(())
    }

    /// Allocate `count` contiguous order-0 frames starting at an `align`-aligned frame and ending
    /// below `max_frame` (exclusive), for the kernel's DMA path (A5c).
    ///
    /// Every frame is taken as an independent order-0 block, so the caller can free the run frame
    /// by frame with [`free`](Self::free) exactly as the existing DMA teardown does. A candidate
    /// run that hits an allocated or reserved frame is rolled back and the next aligned run is
    /// tried.
    ///
    /// # Errors
    ///
    /// [`BuddyError::ZeroSize`], [`BuddyError::Misaligned`] and [`BuddyError::OutOfFrames`].
    pub fn alloc_contiguous(
        &mut self,
        count: u64,
        align: u64,
        max_frame: u64,
    ) -> Result<u64, BuddyError> {
        if count == 0 {
            return Err(BuddyError::ZeroSize);
        }
        if align == 0 || !align.is_power_of_two() {
            return Err(BuddyError::Misaligned);
        }
        let managed_end = self.base_frame.saturating_add(self.frame_count);
        let limit = max_frame.min(managed_end);
        if count > limit.saturating_sub(self.base_frame) {
            self.alloc_fail = self.alloc_fail.saturating_add(1);
            return Err(BuddyError::OutOfFrames);
        }
        let mut start = self.base_frame;
        let misaligned = start % align;
        if misaligned != 0 {
            start = start
                .checked_add(align - misaligned)
                .ok_or(BuddyError::OutOfRange)?;
        }
        while start.checked_add(count).is_some_and(|end| end <= limit) {
            let saved_allocs = self.allocs;
            let saved_frees = self.frees;
            let mut allocated = 0u64;
            while allocated < count {
                if self.alloc_at(start + allocated, 0).is_err() {
                    break;
                }
                allocated += 1;
            }
            if allocated == count {
                return Ok(start);
            }
            for frame in start..start + allocated {
                let _ = self.free(frame, 0);
            }
            // A failed probe is not a completed allocation: keep the counters meaningful.
            self.allocs = saved_allocs;
            self.frees = saved_frees;
            start = start.checked_add(align).ok_or(BuddyError::OutOfRange)?;
        }
        self.alloc_fail = self.alloc_fail.saturating_add(1);
        Err(BuddyError::OutOfFrames)
    }

    /// Reserve `count` frames starting at `frame` so they are never allocated.
    ///
    /// The range must be entirely free (not allocated) at the time of the call.
    ///
    /// # Errors
    ///
    /// [`BuddyError::ZeroSize`], [`BuddyError::OutOfRange`] and [`BuddyError::Occupied`].
    pub fn reserve(&mut self, frame: u64, count: u64) -> Result<(), BuddyError> {
        if count == 0 {
            return Err(BuddyError::ZeroSize);
        }
        let Some(start) = frame.checked_sub(self.base_frame) else {
            return Err(BuddyError::OutOfRange);
        };
        let end = start.checked_add(count).ok_or(BuddyError::OutOfRange)?;
        if end > self.frame_count {
            return Err(BuddyError::OutOfRange);
        }
        for index in start as usize..end as usize {
            match self.blocks[index].state {
                BlockState::Reserved => {}
                BlockState::Allocated => return Err(BuddyError::Occupied),
                BlockState::Free => {
                    if self.carve(index, 0).is_none() {
                        return Err(BuddyError::Occupied);
                    }
                    self.mark_block(index, 0, BlockState::Reserved);
                    self.reserved = self.reserved.saturating_add(1);
                }
            }
        }
        Ok(())
    }

    /// Number of free blocks at `order`.
    pub fn free_blocks(&self, order: usize) -> u64 {
        if order > self.top_order {
            return 0;
        }
        let mut count = 0u64;
        let mut cursor = self.free_heads[order];
        while cursor != NONE {
            count = count.saturating_add(1);
            cursor = self.blocks[cursor as usize].next_free;
        }
        count
    }

    /// Number of free frames across all orders.
    pub fn free_frames(&self) -> u64 {
        let mut free = 0u64;
        for order in 0..=self.top_order {
            free = free.saturating_add(self.free_blocks(order).saturating_mul(1u64 << order));
        }
        free
    }

    /// Zone accounting.
    pub fn stats(&self) -> BuddyStats {
        BuddyStats {
            total: self.frame_count,
            free: self.free_frames(),
            reserved: self.reserved,
            allocs: self.allocs,
            frees: self.frees,
            alloc_fail: self.alloc_fail,
        }
    }

    // --- internal helpers ---

    /// Mark the block `(index, order)` (and every order-0 block it covers) with `state`.
    fn mark_block(&mut self, index: usize, order: usize, state: BlockState) {
        let size = 1usize << order;
        for block in &mut self.blocks[index..index + size] {
            block.state = state;
            block.order = order as u8;
        }
    }

    fn push_free(&mut self, order: usize, index: usize) {
        self.blocks[index].next_free = self.free_heads[order];
        self.free_heads[order] = index as u32;
    }

    fn pop_free(&mut self, order: usize) -> usize {
        let head = self.free_heads[order];
        debug_assert!(head != NONE);
        let index = head as usize;
        self.free_heads[order] = self.blocks[index].next_free;
        index
    }

    fn remove_from_free(&mut self, order: usize, index: usize) -> bool {
        let mut previous = NONE;
        let mut cursor = self.free_heads[order];
        while cursor != NONE {
            if cursor as usize == index {
                if previous == NONE {
                    self.free_heads[order] = self.blocks[index].next_free;
                } else {
                    self.blocks[previous as usize].next_free = self.blocks[index].next_free;
                }
                return true;
            }
            previous = cursor;
            cursor = self.blocks[cursor as usize].next_free;
        }
        false
    }

    /// Split the free block containing `index` down to `order`, so the block `(index-aligned,
    /// order)` becomes a free block that is not in any free list. Returns its head.
    fn carve(&mut self, index: usize, order: usize) -> Option<usize> {
        let mut found = None;
        for current in (order..=self.top_order).rev() {
            let head = (index >> current) << current;
            if self.blocks[head].state == BlockState::Free
                && self.blocks[head].order == current as u8
            {
                found = Some((head, current));
                break;
            }
        }
        let (mut head, mut current) = found?;
        self.remove_from_free(current, head);
        while current > order {
            let half = 1 << (current - 1);
            let right = head + half;
            if index < right {
                self.mark_block(right, current - 1, BlockState::Free);
                self.push_free(current - 1, right);
                self.mark_block(head, current - 1, BlockState::Free);
            } else {
                self.mark_block(head, current - 1, BlockState::Free);
                self.push_free(current - 1, head);
                self.mark_block(right, current - 1, BlockState::Free);
                head = right;
            }
            current -= 1;
        }
        Some(head)
    }

    /// Merge the free block `(index, order)` with its buddy while the buddy is free.
    fn coalesce(&mut self, mut index: usize, mut order: usize) {
        while order < self.top_order {
            let buddy = index ^ (1 << order);
            if buddy >= self.frame_count as usize {
                break;
            }
            if self.blocks[buddy].state == BlockState::Free
                && self.blocks[buddy].order == order as u8
            {
                self.remove_from_free(order, buddy);
                let merged = index.min(buddy);
                self.mark_block(merged, order + 1, BlockState::Free);
                index = merged;
                order += 1;
            } else {
                break;
            }
        }
        self.push_free(order, index);
    }
}

#[cfg(test)]
#[path = "buddy_ut.rs"]
mod tests;
