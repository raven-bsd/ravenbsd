// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The allocator: general size classes plus named object caches over one arena.

#![forbid(unsafe_code)]

use raven_vmem::{DEFAULT_TAG_CAPACITY, VmemGeneric};

use crate::cache::{Cache, CacheStats};
use crate::error::KmemError;
use crate::size::{SIZE_CLASSES, class_align, classify};

/// Number of named caches beyond the general size classes.
const MAX_NAMED_CACHES: usize = 16;
/// Total cache slots: the general classes plus the named pool.
const TOTAL_CACHES: usize = SIZE_CLASSES.len() + MAX_NAMED_CACHES;

/// Default number of slabs per cache for the host-tested [`Kmem`] alias.
pub const DEFAULT_MAX_SLABS: usize = 128;

/// Handle to a named object cache created with [`Kmem::cache_create`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CacheId(usize);

/// Snapshot of the allocator's counters and accounting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct KmemStats {
    /// Successful general allocations (class and large).
    pub allocs: u64,
    /// Successful general frees.
    pub frees: u64,
    /// Rejected general allocations.
    pub alloc_fail: u64,
    /// Large (non-class) allocations not yet freed.
    pub large_live: u64,
    /// Live caches (general classes plus named caches).
    pub caches: usize,
    /// Slabs across all caches.
    pub slabs: usize,
    /// Objects currently handed out across all caches.
    pub in_use: usize,
    /// Free object slots across all caches.
    pub free_objects: usize,
    /// Bytes wasted by stride rounding across all caches.
    pub waste_bytes: u64,
}

/// The object allocator: general size classes plus named caches over one caller-owned arena.
///
/// The arena is a fixed-tag [`VmemGeneric`] owned by the allocator; `TAGS` bounds its boundary-tag
/// slab and `MAX_SLABS` bounds the slab table of each cache. Allocations are address-based (`u64`);
/// reconstructing a typed pointer from an address is the kernel-integration step, where `unsafe`
/// is permitted. All metadata is fixed inline storage, so [`KmemGeneric::new`] and every allocation
/// path are free of global-allocator calls.
pub struct KmemGeneric<const MAX_SLABS: usize, const TAGS: usize> {
    arena: VmemGeneric<TAGS>,
    caches: [Option<Cache<MAX_SLABS>>; TOTAL_CACHES],
    allocs: u64,
    frees: u64,
    alloc_fail: u64,
    large_live: u64,
}

/// The host-tested allocator: [`DEFAULT_MAX_SLABS`] slabs per cache over a default-capacity arena.
pub type Kmem = KmemGeneric<DEFAULT_MAX_SLABS, DEFAULT_TAG_CAPACITY>;

impl<const MAX_SLABS: usize, const TAGS: usize> KmemGeneric<MAX_SLABS, TAGS> {
    /// Build an allocator over `arena`, creating one cache per general size class.
    pub fn new(arena: VmemGeneric<TAGS>) -> Result<Self, KmemError> {
        let mut kmem = Self {
            arena,
            caches: [None; TOTAL_CACHES],
            allocs: 0,
            frees: 0,
            alloc_fail: 0,
            large_live: 0,
        };
        for (index, &class) in SIZE_CLASSES.iter().enumerate() {
            kmem.caches[index] = Some(Cache::new(class_name(class), class, class_align(class)));
        }
        Ok(kmem)
    }

    /// The source arena, read-only.
    pub fn arena(&self) -> &VmemGeneric<TAGS> {
        &self.arena
    }

    /// The source arena, mutable (used by the kernel to import a grown range).
    pub fn arena_mut(&mut self) -> &mut VmemGeneric<TAGS> {
        &mut self.arena
    }

    /// Allocate `size` bytes aligned to the power-of-two `align`.
    ///
    /// Requests that fit a general size class are served from its cache; everything else goes
    /// straight to the arena. The returned address must be released with [`KmemGeneric::free`]
    /// using the same `size` and `align`.
    ///
    /// # Errors
    ///
    /// [`KmemError::ZeroSize`], [`KmemError::Alignment`] and [`KmemError::Exhausted`].
    pub fn alloc(&mut self, size: usize, align: usize) -> Result<u64, KmemError> {
        if size == 0 {
            self.alloc_fail = self.alloc_fail.saturating_add(1);
            return Err(KmemError::ZeroSize);
        }
        if !align.is_power_of_two() {
            self.alloc_fail = self.alloc_fail.saturating_add(1);
            return Err(KmemError::Alignment);
        }
        let outcome = match classify(size, align) {
            Some(index) => {
                let Some(cache) = self.caches[index].as_mut() else {
                    self.alloc_fail = self.alloc_fail.saturating_add(1);
                    return Err(KmemError::InvalidCache);
                };
                cache.alloc(&mut self.arena)
            }
            None => match self.arena.alloc(size as u64, align as u64) {
                Ok(address) => {
                    self.large_live = self.large_live.saturating_add(1);
                    Ok(address)
                }
                Err(_) => Err(KmemError::Exhausted),
            },
        };
        match outcome {
            Ok(address) => {
                self.allocs = self.allocs.saturating_add(1);
                Ok(address)
            }
            Err(error) => {
                self.alloc_fail = self.alloc_fail.saturating_add(1);
                Err(error)
            }
        }
    }

    /// Release an address from [`KmemGeneric::alloc`] with the same `size` and `align`.
    ///
    /// # Errors
    ///
    /// [`KmemError::ZeroSize`], [`KmemError::Alignment`], [`KmemError::DoubleFree`] and
    /// [`KmemError::UnknownPointer`].
    pub fn free(&mut self, ptr: u64, size: usize, align: usize) -> Result<(), KmemError> {
        if size == 0 {
            return Err(KmemError::ZeroSize);
        }
        if !align.is_power_of_two() {
            return Err(KmemError::Alignment);
        }
        let outcome = match classify(size, align) {
            Some(index) => {
                let Some(cache) = self.caches[index].as_mut() else {
                    return Err(KmemError::InvalidCache);
                };
                cache.free(ptr)
            }
            None => match self.arena.free(ptr, size as u64) {
                Ok(()) => {
                    self.large_live = self.large_live.saturating_sub(1);
                    Ok(())
                }
                Err(_) => Err(KmemError::UnknownPointer),
            },
        };
        if outcome.is_ok() {
            self.frees = self.frees.saturating_add(1);
        }
        outcome
    }

    /// Create a named object cache of `object_size`-byte objects aligned to `align`.
    ///
    /// The first [`SIZE_CLASSES`] slots are the general classes; named caches start after them and
    /// reuse a destroyed cache's slot when one is free.
    ///
    /// # Errors
    ///
    /// [`KmemError::ZeroSize`], [`KmemError::Alignment`] and [`KmemError::Exhausted`] when the
    /// named-cache pool is full.
    pub fn cache_create(
        &mut self,
        name: &'static str,
        object_size: usize,
        align: usize,
    ) -> Result<CacheId, KmemError> {
        if object_size == 0 {
            return Err(KmemError::ZeroSize);
        }
        if !align.is_power_of_two() {
            return Err(KmemError::Alignment);
        }
        let slot = self.caches[SIZE_CLASSES.len()..]
            .iter()
            .position(Option::is_none)
            .map(|offset| SIZE_CLASSES.len() + offset)
            .ok_or(KmemError::Exhausted)?;
        self.caches[slot] = Some(Cache::new(name, object_size, align));
        Ok(CacheId(slot))
    }

    /// Allocate one object from the cache.
    pub fn cache_alloc(&mut self, id: CacheId) -> Result<u64, KmemError> {
        let Some(cache) = self.caches.get_mut(id.0).and_then(Option::as_mut) else {
            return Err(KmemError::InvalidCache);
        };
        cache.alloc(&mut self.arena)
    }

    /// Return one object to the cache.
    pub fn cache_free(&mut self, id: CacheId, ptr: u64) -> Result<(), KmemError> {
        let Some(cache) = self.caches.get_mut(id.0).and_then(Option::as_mut) else {
            return Err(KmemError::InvalidCache);
        };
        cache.free(ptr)
    }

    /// Return every fully-free slab of the cache to the arena. Returns the number reaped.
    pub fn cache_reap(&mut self, id: CacheId) -> Result<usize, KmemError> {
        let Some(cache) = self.caches.get_mut(id.0).and_then(Option::as_mut) else {
            return Err(KmemError::InvalidCache);
        };
        Ok(cache.reap(&mut self.arena))
    }

    /// Destroy a named cache, returning all of its slabs to the arena.
    ///
    /// The general size classes cannot be destroyed.
    pub fn cache_destroy(&mut self, id: CacheId) -> Result<(), KmemError> {
        if id.0 < SIZE_CLASSES.len() || id.0 >= self.caches.len() {
            return Err(KmemError::InvalidCache);
        }
        let Some(cache) = self.caches[id.0].as_mut() else {
            return Err(KmemError::InvalidCache);
        };
        cache.release(&mut self.arena);
        self.caches[id.0] = None;
        Ok(())
    }

    /// The cache's statistics, or `None` for an invalid handle.
    pub fn cache_stats(&self, id: CacheId) -> Option<CacheStats> {
        self.caches.get(id.0)?.as_ref().map(Cache::stats)
    }

    /// Statistics for a general size-class slot (0-based index into `SIZE_CLASSES`).
    pub fn general_stats(&self, index: usize) -> Option<CacheStats> {
        self.caches.get(index)?.as_ref().map(Cache::stats)
    }

    /// Snapshot the allocator's counters and accounting.
    pub fn stats(&self) -> KmemStats {
        let mut stats = KmemStats {
            allocs: self.allocs,
            frees: self.frees,
            alloc_fail: self.alloc_fail,
            large_live: self.large_live,
            ..KmemStats::default()
        };
        for cache in self.caches.iter().flatten() {
            let cache = cache.stats();
            stats.caches += 1;
            stats.slabs += cache.slabs;
            stats.in_use += cache.in_use;
            stats.free_objects += cache.free;
            stats.waste_bytes = stats.waste_bytes.saturating_add(cache.waste_bytes);
        }
        stats
    }
}

/// A short cache name for a general size class.
fn class_name(class: usize) -> &'static str {
    match class {
        8 => "kmem-8",
        16 => "kmem-16",
        24 => "kmem-24",
        32 => "kmem-32",
        48 => "kmem-48",
        64 => "kmem-64",
        96 => "kmem-96",
        128 => "kmem-128",
        192 => "kmem-192",
        256 => "kmem-256",
        384 => "kmem-384",
        512 => "kmem-512",
        768 => "kmem-768",
        1024 => "kmem-1024",
        1536 => "kmem-1536",
        2048 => "kmem-2048",
        3072 => "kmem-3072",
        4096 => "kmem-4096",
        8192 => "kmem-8192",
        _ => "kmem-class",
    }
}

#[cfg(test)]
#[path = "kmem_ut.rs"]
mod tests;
