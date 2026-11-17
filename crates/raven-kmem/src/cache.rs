// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One object cache: slabs carved from the source arena into fixed-size slots.
//!
//! The slab table and each slab's free list are fixed inline arrays, so the cache never allocates
//! through the global allocator — not at construction, not on the allocation fast path, and not
//! when a new slab is carved from the arena. A cache whose slab table is full fails with
//! [`KmemError::Exhausted`] instead of growing.

#![forbid(unsafe_code)]

use raven_vmem::Arena;

use crate::error::KmemError;

/// Target slab size in bytes; a slab is carved into as many objects as fit.
const SLAB_TARGET_BYTES: usize = 4096;

/// Maximum objects in one slab. A slab is at most 4096 bytes and the smallest object the cache
/// serves is 8 bytes, so 512 objects always suffice; a request for a smaller object clamps here.
pub(crate) const MAX_OBJECTS_PER_SLAB: usize = 512;
/// 64-bit words in the per-slab free bitmap.
const FREE_WORDS: usize = MAX_OBJECTS_PER_SLAB / 64;

/// A slab: one contiguous arena span divided into `objects_per_slab` equal slots, with a fixed
/// free bitmap (a set bit means the slot is free).
#[derive(Clone, Copy, Debug)]
struct Slab {
    base: u64,
    free_bits: [u64; FREE_WORDS],
    free_count: usize,
}

impl Slab {
    const fn empty() -> Self {
        Self {
            base: 0,
            free_bits: [0; FREE_WORDS],
            free_count: 0,
        }
    }

    /// Mark slot `index` free.
    fn mark_free(&mut self, index: u32) {
        let i = index as usize;
        self.free_bits[i / 64] |= 1 << (i % 64);
        self.free_count += 1;
    }

    /// Take a free slot (the lowest one). The caller must guarantee `free_count > 0`.
    fn alloc(&mut self) -> u32 {
        for (word_index, word) in self.free_bits.iter().enumerate() {
            let trailing = word.trailing_zeros();
            if trailing < 64 {
                let index = (word_index * 64 + trailing as usize) as u32;
                self.free_bits[word_index] &= !(1 << trailing);
                self.free_count -= 1;
                return index;
            }
        }
        unreachable!("alloc from a slab with no free slot")
    }

    /// Return slot `index` to the free pool.
    fn free(&mut self, index: u32) {
        let i = index as usize;
        self.free_bits[i / 64] |= 1 << (i % 64);
        self.free_count += 1;
    }

    fn contains_free(&self, index: u32) -> bool {
        let i = index as usize;
        self.free_bits[i / 64] & (1 << (i % 64)) != 0
    }
}

/// Snapshot of a cache's counters and accounting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheStats {
    /// Cache name.
    pub name: &'static str,
    /// Successful allocations.
    pub allocs: u64,
    /// Successful frees.
    pub frees: u64,
    /// Rejected allocations.
    pub alloc_fail: u64,
    /// Slabs currently attached to the cache.
    pub slabs: usize,
    /// Objects currently handed out.
    pub in_use: usize,
    /// Free object slots.
    pub free: usize,
    /// Bytes wasted by slot-stride rounding inside every slab.
    pub waste_bytes: u64,
    /// Fully-free slabs returned to the arena.
    pub reaped: u64,
}

/// A fixed-size object cache over the source arena.
///
/// Each slab is a contiguous arena allocation cut into `objects_per_slab` slots of `stride` bytes
/// (`stride` is `object_size` rounded up to `align`). A per-slab free list of slot indices makes
/// allocation a pop and free a push, both in safe Rust; there are no intrusive links in object
/// memory, so the cache never writes through a raw pointer.
#[derive(Clone, Copy, Debug)]
pub(super) struct Cache<const MAX_SLABS: usize> {
    name: &'static str,
    object_size: usize,
    align: usize,
    stride: usize,
    objects_per_slab: usize,
    slabs: [Slab; MAX_SLABS],
    slab_count: usize,
    allocs: u64,
    frees: u64,
    alloc_fail: u64,
    reaped: u64,
}

/// Round `value` up to a multiple of the power-of-two `align`.
fn align_up(value: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two());
    value.div_ceil(align) * align
}

impl<const MAX_SLABS: usize> Cache<MAX_SLABS> {
    pub(super) fn new(name: &'static str, object_size: usize, align: usize) -> Self {
        let stride = align_up(object_size, align);
        let objects_per_slab = (SLAB_TARGET_BYTES / stride).clamp(1, MAX_OBJECTS_PER_SLAB);
        Self {
            name,
            object_size,
            align,
            stride,
            objects_per_slab,
            slabs: [Slab::empty(); MAX_SLABS],
            slab_count: 0,
            allocs: 0,
            frees: 0,
            alloc_fail: 0,
            reaped: 0,
        }
    }

    /// Allocate one object, growing the cache with a fresh slab when needed.
    pub(super) fn alloc(&mut self, arena: &mut impl Arena) -> Result<u64, KmemError> {
        if let Some(slab) = self.slabs[..self.slab_count]
            .iter_mut()
            .find(|slab| slab.free_count > 0)
        {
            let index = slab.alloc();
            let address = address_of(slab.base, index, self.stride);
            self.allocs = self.allocs.saturating_add(1);
            return Ok(address);
        }

        if self.slab_count >= MAX_SLABS {
            self.alloc_fail = self.alloc_fail.saturating_add(1);
            return Err(KmemError::Exhausted);
        }
        let bytes = (self.objects_per_slab * self.stride) as u64;
        let base = arena.alloc(bytes, self.align as u64).map_err(|_| {
            self.alloc_fail = self.alloc_fail.saturating_add(1);
            KmemError::Exhausted
        })?;
        let mut slab = Slab {
            base,
            ..Slab::empty()
        };
        for index in 0..self.objects_per_slab as u32 {
            slab.mark_free(index);
        }
        let index = slab.alloc();
        self.slabs[self.slab_count] = slab;
        self.slab_count += 1;
        self.allocs = self.allocs.saturating_add(1);
        Ok(address_of(base, index, self.stride))
    }

    /// Return an object to its slab's free list.
    pub(super) fn free(&mut self, ptr: u64) -> Result<(), KmemError> {
        let stride = self.stride as u64;
        let count = self.objects_per_slab as u64;
        let Some(span) = count.checked_mul(stride) else {
            return Err(KmemError::UnknownPointer);
        };
        for slab in &mut self.slabs[..self.slab_count] {
            if ptr < slab.base {
                continue;
            }
            let offset = ptr - slab.base;
            if offset >= span {
                continue;
            }
            if !offset.is_multiple_of(stride) {
                return Err(KmemError::UnknownPointer);
            }
            let index = (offset / stride) as u32;
            if slab.contains_free(index) {
                return Err(KmemError::DoubleFree);
            }
            slab.free(index);
            self.frees = self.frees.saturating_add(1);
            return Ok(());
        }
        Err(KmemError::UnknownPointer)
    }

    /// Return every fully-free slab to the arena. Returns the number of slabs reaped.
    pub(super) fn reap(&mut self, arena: &mut impl Arena) -> usize {
        let bytes = (self.objects_per_slab * self.stride) as u64;
        let mut removed = 0;
        let mut i = 0;
        while i < self.slab_count {
            if self.slabs[i].free_count == self.objects_per_slab {
                let slab = self.slabs[i];
                self.slabs[i] = self.slabs[self.slab_count - 1];
                self.slab_count -= 1;
                let _ = arena.free(slab.base, bytes);
                removed += 1;
            } else {
                i += 1;
            }
        }
        self.reaped = self.reaped.saturating_add(removed as u64);
        removed
    }

    /// Return every slab to the arena and drop the cache's state.
    pub(super) fn release(&mut self, arena: &mut impl Arena) {
        let bytes = (self.objects_per_slab * self.stride) as u64;
        for i in 0..self.slab_count {
            let _ = arena.free(self.slabs[i].base, bytes);
        }
        self.slab_count = 0;
    }

    pub(super) fn stats(&self) -> CacheStats {
        let slabs = self.slab_count;
        let total_objects = slabs * self.objects_per_slab;
        let free: usize = self.slabs[..slabs].iter().map(|slab| slab.free_count).sum();
        let in_use = total_objects - free;
        let waste_bytes = (self.stride - self.object_size) as u64 * total_objects as u64;
        CacheStats {
            name: self.name,
            allocs: self.allocs,
            frees: self.frees,
            alloc_fail: self.alloc_fail,
            slabs,
            in_use,
            free,
            waste_bytes,
            reaped: self.reaped,
        }
    }
}

/// Compute the address of object `index` inside a slab starting at `base`.
fn address_of(base: u64, index: u32, stride: usize) -> u64 {
    base + u64::from(index) * stride as u64
}

#[cfg(test)]
#[path = "cache_ut.rs"]
mod tests;
