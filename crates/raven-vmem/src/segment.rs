// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Public view of one tracked extent.

/// Whether a [`Segment`] can be handed out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentState {
    /// The extent belongs to the arena's free pool.
    Free,
    /// The extent has been handed out to a caller and not yet freed.
    Allocated,
}

impl SegmentState {
    /// Return `true` for [`SegmentState::Free`].
    pub const fn is_free(self) -> bool {
        matches!(self, Self::Free)
    }
}

/// One contiguous extent tracked by a [`Vmem`](crate::Vmem).
///
/// Segments are the arena's boundary tags exposed for callers that need to name backing ranges:
/// the kernel uses the allocated ones to map or unmap, and `raven-kmem` uses the free ones to
/// decide where a new slab may live. The boundary tags themselves stay private.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Segment {
    base: u64,
    size: u64,
    state: SegmentState,
}

impl Segment {
    pub(crate) const fn new(base: u64, size: u64, state: SegmentState) -> Self {
        Self { base, size, state }
    }

    /// First address in the segment.
    pub const fn base(self) -> u64 {
        self.base
    }

    /// Length of the segment in bytes.
    pub const fn size(self) -> u64 {
        self.size
    }

    /// One past the last address in the segment.
    pub const fn end(self) -> u64 {
        self.base + self.size
    }

    /// Allocation state of the segment.
    pub const fn state(self) -> SegmentState {
        self.state
    }

    /// Return `true` when the segment is free.
    pub const fn is_free(self) -> bool {
        self.state.is_free()
    }
}

#[cfg(test)]
#[path = "segment_ut.rs"]
mod tests;
