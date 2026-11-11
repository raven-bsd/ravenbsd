// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! No-allocation bitmap physical frame allocator.
//!
//! The allocator manages page-frame numbers only; it never dereferences physical memory. One bit
//! per frame is stored in a caller-provided `[u64]` bitmap, where a set bit means "free". `new`
//! starts with every frame reserved; the caller marks usable ranges explicitly.

/// Why a frame could not be freed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FreeError {
    /// The frame is outside the allocator's base/count range.
    OutOfRange,
    /// The frame was already free.
    DoubleFree,
}

/// Bitmap frame allocator.
pub struct FrameAllocator<'a> {
    bitmap: &'a mut [u64],
    base_frame: u64,
    frame_count: u64,
    next_hint: u64,
}

impl<'a> FrameAllocator<'a> {
    /// Create an allocator with every frame reserved.
    ///
    /// `frame_count` is clamped to the bitmap capacity (`bitmap.len() * 64`).
    pub fn new(bitmap: &'a mut [u64], base_frame: u64, frame_count: u64) -> FrameAllocator<'a> {
        bitmap.fill(0);
        let capacity = (bitmap.len() as u64).saturating_mul(64);
        FrameAllocator {
            bitmap,
            base_frame,
            frame_count: frame_count.min(capacity),
            next_hint: 0,
        }
    }

    /// Mark `count` frames starting at `start_frame` usable.
    pub fn mark_usable(&mut self, start_frame: u64, count: u64) {
        self.mark(start_frame, count, true);
    }

    /// Mark `count` frames starting at `start_frame` reserved.
    pub fn mark_reserved(&mut self, start_frame: u64, count: u64) {
        self.mark(start_frame, count, false);
    }

    /// Allocate one frame, first-fit from the rolling hint.
    pub fn alloc(&mut self) -> Option<u64> {
        if self.frame_count == 0 {
            return None;
        }
        let mut scanned = 0u64;
        while scanned < self.frame_count {
            let relative = (self.next_hint + scanned) % self.frame_count;
            if self.is_free(relative) {
                self.set_free(relative, false);
                self.next_hint = (relative + 1) % self.frame_count;
                return Some(self.base_frame + relative);
            }
            scanned += 1;
        }
        None
    }

    /// Allocate `n` contiguous frames whose absolute first frame is `align_frames`-aligned.
    pub fn alloc_contiguous(&mut self, n: u64, align_frames: u64) -> Option<u64> {
        self.alloc_contiguous_bounded(
            n,
            align_frames,
            self.base_frame.saturating_add(self.frame_count),
        )
    }

    /// Allocate `n` contiguous frames whose absolute first frame is `align_frames`-aligned and
    /// whose last frame is below `max_frame`.
    ///
    /// The upper bound is inclusive of `max_frame - 1`: the whole run lies in
    /// `[base_frame, max_frame)`. This is how callers express device DMA masks.
    pub fn alloc_contiguous_bounded(
        &mut self,
        n: u64,
        align_frames: u64,
        max_frame: u64,
    ) -> Option<u64> {
        if n == 0 || align_frames == 0 || n > self.frame_count {
            return None;
        }
        // The highest absolute start that still leaves every frame below `max_frame`.
        let max_start = max_frame.checked_sub(n)?;
        let mut start = 0u64;
        while start.checked_add(n)? <= self.frame_count {
            let absolute = self.base_frame.checked_add(start)?;
            if absolute <= max_start && absolute % align_frames == 0 {
                let mut offset = 0u64;
                while offset < n && self.is_free(start + offset) {
                    offset += 1;
                }
                if offset == n {
                    let mut offset = 0u64;
                    while offset < n {
                        self.set_free(start + offset, false);
                        offset += 1;
                    }
                    self.next_hint = (start + n) % self.frame_count;
                    return Some(absolute);
                }
            }
            start += 1;
        }
        None
    }

    /// Free one frame; double frees and unknown frames are errors.
    pub fn free(&mut self, frame: u64) -> Result<(), FreeError> {
        let relative = frame
            .checked_sub(self.base_frame)
            .ok_or(FreeError::OutOfRange)?;
        if relative >= self.frame_count {
            return Err(FreeError::OutOfRange);
        }
        if self.is_free(relative) {
            return Err(FreeError::DoubleFree);
        }
        self.set_free(relative, true);
        Ok(())
    }

    /// Number of currently free frames.
    pub fn free_frames(&self) -> u64 {
        let mut free = 0u64;
        let mut frame = 0u64;
        while frame < self.frame_count {
            if self.is_free(frame) {
                free += 1;
            }
            frame += 1;
        }
        free
    }

    /// Number of managed frames.
    pub fn total_frames(&self) -> u64 {
        self.frame_count
    }

    fn mark(&mut self, start_frame: u64, count: u64, free: bool) {
        if count == 0 {
            return;
        }
        let Some(relative) = start_frame.checked_sub(self.base_frame) else {
            return;
        };
        if relative >= self.frame_count {
            return;
        }
        let count = count.min(self.frame_count - relative);
        let mut offset = 0u64;
        while offset < count {
            self.set_free(relative + offset, free);
            offset += 1;
        }
    }

    fn is_free(&self, relative: u64) -> bool {
        let word = (relative / 64) as usize;
        let bit = (relative % 64) as u32;
        self.bitmap
            .get(word)
            .is_some_and(|value| value & (1u64 << bit) != 0)
    }

    fn set_free(&mut self, relative: u64, free: bool) {
        let word = (relative / 64) as usize;
        let bit = (relative % 64) as u32;
        if let Some(value) = self.bitmap.get_mut(word) {
            if free {
                *value |= 1u64 << bit;
            } else {
                *value &= !(1u64 << bit);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameAllocator, FreeError};
    use std::collections::BTreeSet;

    fn allocator<'a>(bitmap: &'a mut [u64], frames: u64) -> FrameAllocator<'a> {
        let mut allocator = FrameAllocator::new(bitmap, 0, frames);
        allocator.mark_usable(0, frames);
        allocator
    }

    fn xorshift(state: &mut u64) -> u64 {
        let mut value = *state;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        *state = value;
        value
    }

    #[test]
    fn new_starts_reserved() {
        let mut bitmap = [u64::MAX; 2];
        let mut allocator = FrameAllocator::new(&mut bitmap, 10, 128);
        assert_eq!(allocator.total_frames(), 128);
        assert_eq!(allocator.free_frames(), 0);
        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn alloc_and_free_cycle() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        for frame in 0..128 {
            assert_eq!(allocator.alloc(), Some(frame));
        }
        assert_eq!(allocator.alloc(), None);
        assert_eq!(allocator.free(64), Ok(()));
        assert_eq!(allocator.alloc(), Some(64));
    }

    #[test]
    fn double_free_is_detected() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        let frame = allocator.alloc().unwrap();
        assert_eq!(allocator.free(frame), Ok(()));
        assert_eq!(allocator.free(frame), Err(FreeError::DoubleFree));
    }

    #[test]
    fn out_of_range_free_is_rejected() {
        let mut bitmap = [0u64; 1];
        let mut allocator = allocator(&mut bitmap, 64);
        assert_eq!(allocator.free(64), Err(FreeError::OutOfRange));
    }

    #[test]
    fn base_frame_is_honoured() {
        let mut bitmap = [0u64; 1];
        let mut allocator = FrameAllocator::new(&mut bitmap, 100, 32);
        allocator.mark_usable(100, 32);
        assert_eq!(allocator.alloc(), Some(100));
        assert_eq!(allocator.alloc_contiguous(1, 8), Some(104));
    }

    #[test]
    fn contiguous_alignment_is_honoured() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        allocator.mark_reserved(0, 1);
        assert_eq!(allocator.alloc_contiguous(2, 4), Some(4));
    }

    #[test]
    fn contiguous_requires_a_large_enough_hole() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        for frame in 0..126 {
            allocator.mark_reserved(frame, 1);
        }
        assert_eq!(allocator.alloc_contiguous(4, 1), None);
        assert_eq!(allocator.alloc_contiguous(2, 1), Some(126));
    }

    #[test]
    fn exhaustion_returns_none() {
        let mut bitmap = [0u64; 1];
        let mut allocator = allocator(&mut bitmap, 8);
        for _ in 0..8 {
            assert!(allocator.alloc().is_some());
        }
        assert_eq!(allocator.alloc(), None);
        assert_eq!(allocator.alloc_contiguous(1, 1), None);
    }

    #[test]
    fn reserved_overlap_is_not_handed_out() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        allocator.mark_reserved(10, 10);
        for _ in 0..10 {
            let frame = allocator.alloc().unwrap();
            assert!(!(10..20).contains(&frame));
        }
    }

    #[test]
    fn fragmentation_and_hint_wrap() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        for frame in 0..128 {
            if frame % 2 == 0 {
                allocator.mark_reserved(frame, 1);
            }
        }
        let first = allocator.alloc().unwrap();
        assert_eq!(first, 1);
        for _ in 0..63 {
            assert!(allocator.alloc().is_some());
        }
        assert_eq!(allocator.alloc(), None);
    }

    #[test]
    fn free_frames_tracks_changes() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 100);
        assert_eq!(allocator.free_frames(), 100);
        let frame = allocator.alloc().unwrap();
        assert_eq!(allocator.free_frames(), 99);
        allocator.free(frame).unwrap();
        assert_eq!(allocator.free_frames(), 100);
    }

    #[test]
    fn zero_frames_is_empty() {
        let mut bitmap = [u64::MAX; 1];
        let allocator = FrameAllocator::new(&mut bitmap, 0, 0);
        assert_eq!(allocator.total_frames(), 0);
        assert_eq!(allocator.free_frames(), 0);
    }

    #[test]
    fn bitmap_capacity_clamps_frame_count() {
        let mut bitmap = [0u64; 1];
        let allocator = FrameAllocator::new(&mut bitmap, 0, 1024);
        assert_eq!(allocator.total_frames(), 64);
    }

    #[test]
    fn marking_outside_the_range_is_ignored() {
        let mut bitmap = [0u64; 1];
        let mut allocator = FrameAllocator::new(&mut bitmap, 100, 8);
        allocator.mark_usable(100, 8);
        allocator.mark_usable(0, 1000);
        allocator.mark_usable(108, 4);
        allocator.mark_reserved(100, 1);
        assert_eq!(allocator.free_frames(), 7);
    }

    #[test]
    fn contiguous_with_zero_arguments_returns_none() {
        let mut bitmap = [0u64; 1];
        let mut allocator = allocator(&mut bitmap, 64);
        assert_eq!(allocator.alloc_contiguous(0, 1), None);
        assert_eq!(allocator.alloc_contiguous(1, 0), None);
    }

    #[test]
    fn bounded_run_stays_below_the_bound() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        let start = allocator.alloc_contiguous_bounded(4, 1, 8).unwrap();
        assert!(start < 8);
        assert!(start + 4 <= 8);
    }

    #[test]
    fn bounded_run_may_end_exactly_at_the_bound() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        assert_eq!(allocator.alloc_contiguous_bounded(4, 1, 4), Some(0));
    }

    #[test]
    fn bounded_run_crossing_the_bound_returns_none() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        assert_eq!(allocator.alloc_contiguous_bounded(4, 1, 3), None);
    }

    #[test]
    fn bounded_run_larger_than_the_bound_returns_none() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        assert_eq!(allocator.alloc_contiguous_bounded(8, 1, 4), None);
    }

    #[test]
    fn bounded_zero_max_frame_returns_none() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        assert_eq!(allocator.alloc_contiguous_bounded(1, 1, 0), None);
    }

    #[test]
    fn bounded_zero_arguments_return_none() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        assert_eq!(allocator.alloc_contiguous_bounded(0, 1, 128), None);
        assert_eq!(allocator.alloc_contiguous_bounded(1, 0, 128), None);
    }

    #[test]
    fn bounded_honours_alignment_inside_the_bound() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        allocator.mark_reserved(0, 1);
        assert_eq!(allocator.alloc_contiguous_bounded(2, 4, 16), Some(4));
    }

    #[test]
    fn bounded_skips_an_alignment_that_crosses_the_bound() {
        let mut first_bitmap = [0u64; 2];
        let mut first = allocator(&mut first_bitmap, 128);
        first.mark_reserved(0, 8);
        // Only aligned starts at or below 20-8=12 remain; 8 works.
        assert_eq!(first.alloc_contiguous_bounded(8, 8, 20), Some(8));

        let mut second_bitmap = [0u64; 2];
        let mut second = allocator(&mut second_bitmap, 128);
        second.mark_reserved(0, 8);
        // Bound 15 allows only the reserved start 0: no run.
        assert_eq!(second.alloc_contiguous_bounded(8, 8, 15), None);
    }

    #[test]
    fn bounded_skips_holes_inside_the_bound() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        allocator.mark_reserved(2, 2);
        // Only starts at or below 6-4=2 are allowed; every 4-frame window there hits the hole.
        assert_eq!(allocator.alloc_contiguous_bounded(4, 1, 6), None);
        assert_eq!(allocator.alloc_contiguous_bounded(4, 1, 128), Some(4));
    }

    #[test]
    fn bounded_marks_the_run_allocated() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        let start = allocator.alloc_contiguous_bounded(3, 1, 64).unwrap();
        assert_eq!(allocator.alloc_contiguous_bounded(3, 1, 64), Some(3));
        for frame in start..start + 3 {
            assert_eq!(allocator.free(frame), Ok(()));
        }
        assert_eq!(allocator.alloc_contiguous_bounded(3, 1, 64), Some(start));
    }

    #[test]
    fn bounded_exhaustion_returns_none() {
        let mut bitmap = [0u64; 1];
        let mut allocator = allocator(&mut bitmap, 64);
        allocator.mark_reserved(8, 56);
        assert_eq!(allocator.alloc_contiguous_bounded(8, 1, 8), Some(0));
        assert_eq!(allocator.alloc_contiguous_bounded(1, 1, 8), None);
    }

    #[test]
    fn bounded_free_coalesces_adjacent_runs() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        let first = allocator.alloc_contiguous_bounded(2, 1, 128).unwrap();
        let second = allocator.alloc_contiguous_bounded(2, 1, 128).unwrap();
        assert_eq!(allocator.free(first), Ok(()));
        assert_eq!(allocator.free(first + 1), Ok(()));
        assert_eq!(allocator.free(second), Ok(()));
        assert_eq!(allocator.free(second + 1), Ok(()));
        // The whole four-frame span must be reallocatable as one run.
        assert_eq!(allocator.alloc_contiguous_bounded(4, 1, 128), Some(first));
    }

    #[test]
    fn bounded_no_double_allocation_of_the_same_frame() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        let first = allocator.alloc_contiguous_bounded(1, 1, 128).unwrap();
        let _ = allocator.alloc_contiguous_bounded(1, 1, 128).unwrap();
        assert_ne!(allocator.alloc_contiguous_bounded(1, 1, 128), Some(first));
    }

    #[test]
    fn bounded_honours_the_base_frame() {
        let mut bitmap = [0u64; 1];
        let mut allocator = FrameAllocator::new(&mut bitmap, 100, 32);
        allocator.mark_usable(100, 32);
        assert_eq!(allocator.alloc_contiguous_bounded(4, 1, 104), Some(100));
        assert_eq!(allocator.alloc_contiguous_bounded(4, 1, 103), None);
    }

    #[test]
    fn bounded_updates_the_rolling_hint() {
        let mut bitmap = [0u64; 2];
        let mut allocator = allocator(&mut bitmap, 128);
        let start = allocator.alloc_contiguous_bounded(2, 1, 128).unwrap();
        assert_eq!(allocator.alloc(), Some(start + 2));
    }

    #[test]
    fn bounded_matches_unbounded_when_the_bound_is_total() {
        let mut first_bitmap = [0u64; 4];
        let mut first = allocator(&mut first_bitmap, 256);
        let mut second_bitmap = [0u64; 4];
        let mut second = allocator(&mut second_bitmap, 256);
        assert_eq!(
            first.alloc_contiguous(5, 4),
            second.alloc_contiguous_bounded(5, 4, 256)
        );
    }

    #[test]
    fn bounded_random_model_never_hands_out_frames_twice() {
        let mut bitmap = [0u64; 8];
        let mut allocator = allocator(&mut bitmap, 512);
        let mut free: BTreeSet<u64> = (0..512).collect();
        let mut allocated: BTreeSet<u64> = BTreeSet::new();
        let mut state = 0x0BAD_5EED_F00D_CAFEu64;
        for _ in 0..2000 {
            let n = xorshift(&mut state) % 8 + 1;
            let align = 1u64 << (xorshift(&mut state) % 4);
            let max = 512 - xorshift(&mut state) % 64;
            let Some(start) = allocator.alloc_contiguous_bounded(n, align, max) else {
                continue;
            };
            assert!(start.saturating_add(n) <= max);
            assert_eq!(start % align, 0);
            for frame in start..start + n {
                assert!(free.remove(&frame), "double allocation of {frame}");
                assert!(allocated.insert(frame));
            }
            if xorshift(&mut state) & 1 == 0 {
                for frame in start..start + n {
                    assert_eq!(allocator.free(frame), Ok(()));
                    allocated.remove(&frame);
                    free.insert(frame);
                }
            }
        }
        while let Some(frame) = allocator.alloc() {
            assert!(free.remove(&frame));
            assert!(allocated.insert(frame));
        }
        assert!(free.is_empty());
        assert_eq!(allocated.len(), 512);
    }

    #[test]
    fn random_alloc_free_sequence_matches_model() {
        let mut bitmap = [0u64; 8];
        let mut allocator = allocator(&mut bitmap, 512);
        let mut free: BTreeSet<u64> = (0..512).collect();
        let mut allocated: BTreeSet<u64> = BTreeSet::new();
        let mut state = 0x5EED_1234_ABCD_9876u64;

        for _ in 0..5000 {
            if xorshift(&mut state) & 1 == 0 {
                let got = allocator.alloc();
                if free.is_empty() {
                    assert_eq!(got, None);
                } else {
                    let frame = got.expect("free frame available");
                    assert!(free.remove(&frame), "duplicate allocation {frame}");
                    assert!(allocated.insert(frame), "model inconsistency {frame}");
                }
            } else {
                let frame = xorshift(&mut state) % 512;
                if allocated.remove(&frame) {
                    free.insert(frame);
                    assert_eq!(allocator.free(frame), Ok(()));
                } else if free.contains(&frame) {
                    assert_eq!(allocator.free(frame), Err(FreeError::DoubleFree));
                }
            }
            assert_eq!(allocator.free_frames(), free.len() as u64);
        }

        while let Some(frame) = allocator.alloc() {
            assert!(free.remove(&frame), "model lost frame {frame}");
            assert!(allocated.insert(frame));
        }
        assert!(free.is_empty());
        assert_eq!(allocated.len(), 512);
    }
}
