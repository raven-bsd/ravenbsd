// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! The allocator: general size classes plus named object caches over one arena.

#![forbid(unsafe_code)]

use alloc::vec::Vec;

use raven_vmem::Vmem;

use crate::cache::{Cache, CacheStats};
use crate::error::KmemError;
use crate::size::{SIZE_CLASSES, class_align, classify};

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

/// The object allocator: general size classes plus named caches over one [`Vmem`] arena.
///
/// The arena is caller-supplied and owned by the [`Kmem`]. Allocations are address-based (`u64`);
/// reconstructing a typed pointer from an address is the kernel-integration (A4) step, where
/// `unsafe` is permitted. This crate hands out and reclaims raw addresses only.
pub struct Kmem {
    arena: Vmem,
    caches: Vec<Option<Cache>>,
    allocs: u64,
    frees: u64,
    alloc_fail: u64,
    large_live: u64,
}

impl Kmem {
    /// Build an allocator over `arena`, creating one cache per general size class.
    pub fn new(arena: Vmem) -> Result<Self, KmemError> {
        let mut caches: Vec<Option<Cache>> = Vec::new();
        caches
            .try_reserve_exact(SIZE_CLASSES.len())
            .map_err(|_| KmemError::NoMemory)?;
        for &class in &SIZE_CLASSES {
            caches.push(Some(Cache::new(
                class_name(class),
                class,
                class_align(class),
            )));
        }
        Ok(Self {
            arena,
            caches,
            allocs: 0,
            frees: 0,
            alloc_fail: 0,
            large_live: 0,
        })
    }

    /// The source arena, read-only.
    pub fn arena(&self) -> &Vmem {
        &self.arena
    }

    /// Allocate `size` bytes aligned to the power-of-two `align`.
    ///
    /// Requests that fit a general size class are served from its cache; everything else goes
    /// straight to the arena. The returned address must be released with [`Kmem::free`] using the
    /// same `size` and `align`.
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

    /// Release an address from [`Kmem::alloc`] with the same `size` and `align`.
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
        let cache = Cache::new(name, object_size, align);
        if let Some(index) = self.caches.iter().position(Option::is_none) {
            self.caches[index] = Some(cache);
            Ok(CacheId(index))
        } else {
            self.caches.push(Some(cache));
            Ok(CacheId(self.caches.len() - 1))
        }
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
        if id.0 < SIZE_CLASSES.len() {
            return Err(KmemError::InvalidCache);
        }
        let Some(cache) = self.caches.get_mut(id.0).and_then(Option::as_mut) else {
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
