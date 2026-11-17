// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! One object cache: slabs carved from the source arena into fixed-size slots.

#![forbid(unsafe_code)]

use alloc::vec::Vec;

use raven_vmem::Vmem;

use crate::error::KmemError;

/// Target slab size in bytes; a slab is carved into as many objects as fit.
const SLAB_TARGET_BYTES: usize = 4096;

/// A slab: one contiguous arena span divided into `objects_per_slab` equal slots.
struct Slab {
    base: u64,
    free: Vec<u32>,
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
pub(super) struct Cache {
    name: &'static str,
    object_size: usize,
    align: usize,
    stride: usize,
    objects_per_slab: usize,
    slabs: Vec<Slab>,
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

impl Cache {
    pub(super) fn new(name: &'static str, object_size: usize, align: usize) -> Self {
        let stride = align_up(object_size, align);
        let objects_per_slab = (SLAB_TARGET_BYTES / stride).max(1);
        Self {
            name,
            object_size,
            align,
            stride,
            objects_per_slab,
            slabs: Vec::new(),
            allocs: 0,
            frees: 0,
            alloc_fail: 0,
            reaped: 0,
        }
    }

    /// Allocate one object, growing the cache with a fresh slab when needed.
    pub(super) fn alloc(&mut self, arena: &mut Vmem) -> Result<u64, KmemError> {
        if let Some(slab) = self.slabs.iter_mut().find(|slab| !slab.free.is_empty()) {
            let index = slab.free.pop().expect("non-empty free list");
            let address = address_of(slab.base, index, self.stride);
            self.allocs = self.allocs.saturating_add(1);
            return Ok(address);
        }

        let bytes = (self.objects_per_slab * self.stride) as u64;
        let base = arena.alloc(bytes, self.align as u64).map_err(|_| {
            self.alloc_fail = self.alloc_fail.saturating_add(1);
            KmemError::Exhausted
        })?;
        let mut free: Vec<u32> = (0..self.objects_per_slab as u32).collect();
        let index = free.pop().expect("slab has at least one object");
        self.slabs.push(Slab { base, free });
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
        for slab in &mut self.slabs {
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
            if slab.free.contains(&index) {
                return Err(KmemError::DoubleFree);
            }
            slab.free.push(index);
            self.frees = self.frees.saturating_add(1);
            return Ok(());
        }
        Err(KmemError::UnknownPointer)
    }

    /// Return every fully-free slab to the arena. Returns the number of slabs reaped.
    pub(super) fn reap(&mut self, arena: &mut Vmem) -> usize {
        let bytes = (self.objects_per_slab * self.stride) as u64;
        let mut removed = 0;
        let mut i = 0;
        while i < self.slabs.len() {
            if self.slabs[i].free.len() == self.objects_per_slab {
                let slab = self.slabs.swap_remove(i);
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
    pub(super) fn release(&mut self, arena: &mut Vmem) {
        let bytes = (self.objects_per_slab * self.stride) as u64;
        for slab in self.slabs.drain(..) {
            let _ = arena.free(slab.base, bytes);
        }
    }

    pub(super) fn stats(&self) -> CacheStats {
        let slabs = self.slabs.len();
        let total_objects = slabs * self.objects_per_slab;
        let free: usize = self.slabs.iter().map(|slab| slab.free.len()).sum();
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
