// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Error and policy types shared by every arena operation.

/// Why an arena operation was rejected.
///
/// Every variant is an ordinary return value: the allocator never panics on a request and never
/// partially applies a failed operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmemError {
    /// The requested length is zero.
    ZeroLength,
    /// An address-plus-length computation does not fit in `u64`.
    Overflow,
    /// An alignment is zero, not a power of two, or an `alloc_at` address is not aligned.
    Alignment,
    /// A free span large enough for the request does not exist.
    Exhausted,
    /// The arena's fixed tag slab has no free record for the split.
    NoTags,
    /// The fixed tag slab could not be allocated when the arena was constructed.
    Capacity,
    /// The imported or placed range overlaps an extent that is already tracked.
    Overlap,
    /// The address (or range) is outside every imported span, or crosses a span boundary.
    OutOfRange,
    /// No segment starts at the freed address.
    UnknownSegment,
    /// The segment at the freed address is already free.
    DoubleFree,
    /// The segment at the freed address has a different length.
    SizeMismatch,
}

/// Search order used when several free spans can satisfy an allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Policy {
    /// Pick the free span that leaves the least free remainder. This is the default.
    #[default]
    BestFit,
    /// Pick the lowest-address free span that fits.
    FirstFit,
    /// Pick the first fit at or above the previous allocation's end, then wrap.
    NextFit,
}
