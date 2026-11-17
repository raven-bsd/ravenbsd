// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Error type shared by the caches and the allocator.

/// Why a kmem operation failed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KmemError {
    /// The requested length is zero.
    ZeroSize,
    /// The alignment is zero or not a power of two.
    Alignment,
    /// The source arena could not satisfy a slab or a large allocation.
    Exhausted,
    /// The cache handle is out of range or points at a destroyed cache.
    InvalidCache,
    /// The pointer does not belong to the cache it was freed through.
    UnknownPointer,
    /// The pointer was already freed.
    DoubleFree,
    /// The slab or bookkeeping storage could not be allocated.
    NoMemory,
}

impl From<raven_vmem::VmemError> for KmemError {
    fn from(error: raven_vmem::VmemError) -> Self {
        use raven_vmem::VmemError;
        match error {
            VmemError::Exhausted | VmemError::NoTags | VmemError::Overflow => KmemError::Exhausted,
            VmemError::ZeroLength => KmemError::ZeroSize,
            VmemError::Alignment => KmemError::Alignment,
            _ => KmemError::NoMemory,
        }
    }
}

#[cfg(test)]
#[path = "error_ut.rs"]
mod tests;
