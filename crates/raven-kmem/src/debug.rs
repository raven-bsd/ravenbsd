// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Opt-in debug allocator: redzones, freed-pattern poison and an allocation audit ring.
//!
//! Enabled by the crate's `debug` feature. The default build compiles none of this, so a release
//! allocator carries no per-allocation cost. This module is the crate's second `unsafe` surface
//! (besides the [`GlobalAlloc`](core::alloc::GlobalAlloc) shim): the redzone and poison patterns
//! are written to the returned object bytes, which is exactly the raw-pointer work the feature
//! exists for. It is host-only; the kernel heap keeps its unchanged behavior.

use crate::error::KmemError;
use crate::kmem::{KmemGeneric, KmemStats};

/// Guard bytes on each side of a debug object (rounded up to the alignment).
pub const REDZONE_BYTES: usize = 8;
/// Redzone fill byte.
const REDZONE_FILL: u8 = 0xa5;
/// Freed-object fill byte.
const POISON_FREE: u8 = 0xde;
/// Fresh-object fill byte (uninitialised-read detector).
const POISON_ALLOC: u8 = 0xba;
/// Audit ring capacity.
pub const AUDIT_CAPACITY: usize = 256;

/// What an audit record describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuditOp {
    /// The address was handed out.
    Alloc,
    /// The address was returned.
    Free,
}

/// One allocation event in the audit ring.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AuditRecord {
    /// The operation.
    pub op: AuditOp,
    /// The object address.
    pub addr: u64,
    /// The requested size.
    pub size: usize,
    /// A monotonically increasing sequence number.
    pub seq: u64,
}

impl AuditRecord {
    const fn empty() -> Self {
        Self {
            op: AuditOp::Alloc,
            addr: 0,
            size: 0,
            seq: 0,
        }
    }
}

/// Detections accumulated by the debug allocator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DebugReport {
    /// Successful allocations.
    pub allocs: u64,
    /// Successful frees.
    pub frees: u64,
    /// Frees whose trailing redzone was corrupted (overflow).
    pub redzone_overflow: u64,
    /// Frees whose leading redzone was corrupted (underflow).
    pub redzone_underflow: u64,
    /// Frees of an already-free object.
    pub double_free: u64,
    /// Reallocations that moved.
    pub reallocs_moved: u64,
}

/// A redzone + poison + audit allocator over a [`KmemGeneric`].
///
/// Each object is handed out as `base + guard`, where `guard` is [`REDZONE_BYTES`] rounded up to
/// the alignment. The leading and trailing `guard` bytes hold a fixed pattern checked on [`free`],
/// so a buffer overflow or underflow is detected; freed objects are overwritten with a poison
/// byte so use-after-free reads are visible.
pub struct DebugAlloc<const MAX_SLABS: usize, const TAGS: usize> {
    kmem: KmemGeneric<MAX_SLABS, TAGS>,
    records: [AuditRecord; AUDIT_CAPACITY],
    head: usize,
    len: usize,
    seq: u64,
    report: DebugReport,
}

impl<const MAX_SLABS: usize, const TAGS: usize> DebugAlloc<MAX_SLABS, TAGS> {
    /// Build a debug allocator over `arena`.
    pub fn new(arena: raven_vmem::VmemGeneric<TAGS>) -> Result<Self, KmemError> {
        Ok(Self {
            kmem: KmemGeneric::new(arena)?,
            records: [AuditRecord::empty(); AUDIT_CAPACITY],
            head: 0,
            len: 0,
            seq: 0,
            report: DebugReport::default(),
        })
    }

    /// Allocate `size` bytes aligned to the power-of-two `align`, guarded on both sides.
    pub fn alloc(&mut self, size: usize, align: usize) -> Result<u64, KmemError> {
        if size == 0 {
            return Err(KmemError::ZeroSize);
        }
        let guard = guard_bytes(align);
        let total = size.checked_add(2 * guard).ok_or(KmemError::Exhausted)?;
        let base = self.kmem.alloc(total, align)?;
        // SAFETY: `base` is a freshly-allocated `total`-byte range; the guard and object bytes
        // written here are within it.
        unsafe {
            core::ptr::write_bytes(base as *mut u8, REDZONE_FILL, guard);
            core::ptr::write_bytes(
                (base + (guard + size) as u64) as *mut u8,
                REDZONE_FILL,
                guard,
            );
            core::ptr::write_bytes((base + guard as u64) as *mut u8, POISON_ALLOC, size);
        }
        let addr = base + guard as u64;
        self.record(AuditOp::Alloc, addr, size);
        self.report.allocs = self.report.allocs.saturating_add(1);
        Ok(addr)
    }

    /// Return an object previously returned by [`DebugAlloc::alloc`].
    pub fn free(&mut self, addr: u64, size: usize, align: usize) -> Result<(), KmemError> {
        if size == 0 {
            return Err(KmemError::ZeroSize);
        }
        let guard = guard_bytes(align);
        let base = addr - guard as u64;
        // SAFETY: `addr` and its guard bytes are a live object previously returned by `alloc`.
        let (leading_ok, trailing_ok) = unsafe {
            (
                all_equal(base, REDZONE_FILL, guard),
                all_equal(base + (guard + size) as u64, REDZONE_FILL, guard),
            )
        };
        if !leading_ok {
            self.report.redzone_underflow = self.report.redzone_underflow.saturating_add(1);
        }
        if !trailing_ok {
            self.report.redzone_overflow = self.report.redzone_overflow.saturating_add(1);
        }
        let total = size + 2 * guard;
        // SAFETY: the whole `total`-byte range is live until the free below.
        unsafe {
            core::ptr::write_bytes(base as *mut u8, POISON_FREE, total);
        }
        match self.kmem.free(base, total, align) {
            Ok(()) => {}
            Err(KmemError::DoubleFree) => {
                self.report.double_free = self.report.double_free.saturating_add(1);
                return Err(KmemError::DoubleFree);
            }
            Err(error) => return Err(error),
        }
        self.record(AuditOp::Free, addr, size);
        self.report.frees = self.report.frees.saturating_add(1);
        Ok(())
    }

    /// Grow or shrink an allocation, preserving up to `size` bytes.
    pub fn realloc(
        &mut self,
        addr: u64,
        size: usize,
        new_size: usize,
        align: usize,
    ) -> Result<u64, KmemError> {
        if new_size == 0 {
            self.free(addr, size, align)?;
            return Ok(0);
        }
        let new_addr = self.alloc(new_size, align)?;
        // SAFETY: both ranges are live objects; the copy length is the smaller of the two.
        unsafe {
            core::ptr::copy_nonoverlapping(
                addr as *const u8,
                new_addr as *mut u8,
                core::cmp::min(size, new_size),
            );
        }
        self.free(addr, size, align)?;
        self.report.reallocs_moved = self.report.reallocs_moved.saturating_add(1);
        Ok(new_addr)
    }

    /// Number of audit records available, oldest first.
    pub fn audit_len(&self) -> usize {
        self.len
    }

    /// The `index`-th audit record (0-based, oldest first).
    pub fn audit_record(&self, index: usize) -> AuditRecord {
        debug_assert!(index < self.len);
        self.records[(self.head + index) % AUDIT_CAPACITY]
    }

    /// The accumulated detections.
    pub fn report(&self) -> DebugReport {
        self.report
    }

    /// The underlying allocator's statistics.
    pub fn stats(&self) -> KmemStats {
        self.kmem.stats()
    }

    /// The underlying arena, read-only.
    pub fn arena(&self) -> &raven_vmem::VmemGeneric<TAGS> {
        self.kmem.arena()
    }

    fn record(&mut self, op: AuditOp, addr: u64, size: usize) {
        let record = AuditRecord {
            op,
            addr,
            size,
            seq: self.seq,
        };
        self.seq = self.seq.saturating_add(1);
        if self.len == AUDIT_CAPACITY {
            self.head = (self.head + 1) % AUDIT_CAPACITY;
        } else {
            self.len += 1;
        }
        let index = (self.head + self.len - 1) % AUDIT_CAPACITY;
        self.records[index] = record;
    }
}

/// Guard bytes for a given power-of-two `align` (at least [`REDZONE_BYTES`], aligned).
fn guard_bytes(align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    REDZONE_BYTES.div_ceil(align) * align
}

/// Whether `count` bytes at `addr` all equal `byte`.
///
/// # Safety
/// `addr` must be valid for `count` readable bytes.
unsafe fn all_equal(addr: u64, byte: u8, count: usize) -> bool {
    for offset in 0..count {
        // SAFETY: the caller guarantees `count` readable bytes from `addr`.
        unsafe {
            if *((addr + offset as u64) as *const u8) != byte {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
#[path = "debug_ut.rs"]
mod tests;
