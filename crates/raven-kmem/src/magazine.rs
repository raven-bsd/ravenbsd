// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-CPU magazines and the depot layer over a slab cache.
//!
//! The fast path of [`MagCache::alloc`] and [`MagCache::free`] touches only the current CPU's
//! `loaded` magazine, so a successful pop/push never takes a shared lock and never performs an
//! atomic read-modify-write. Only when a magazine runs empty (alloc) or full (free) does the cache
//! fall to the depot, and only when the depot cannot help does it touch the slab layer. The remote
//! free queue per CPU is bounded and drains into the depot when full. Every buffer is a fixed
//! inline array, so the cache never allocates through the global allocator.

#![forbid(unsafe_code)]

use raven_vmem::Arena;

use crate::cache::Cache;
use crate::error::KmemError;

/// Smallest magazine size (objects per magazine).
pub const ROUNDS_MIN: usize = 15;
/// Largest magazine size.
pub const ROUNDS_MAX: usize = 143;
/// Depot bound as a multiple of the online CPU count.
const DEPOT_MULT: usize = 2;

/// Magazine size for a given number of online CPUs.
///
/// The size starts at [`ROUNDS_MIN`] for one CPU and grows linearly with the CPU count until it
/// caps at [`ROUNDS_MAX`]. This is the "dynamic sizing from the online CPU count" the ticket
/// specifies (the contention-driven doubling in R-MEM-7 is deferred to the kernel integration).
pub fn magazine_rounds(online_cpus: usize) -> usize {
    (ROUNDS_MIN * online_cpus.max(1)).min(ROUNDS_MAX)
}

/// The CPU context an allocation or free runs on.
///
/// The kernel supplies the real current CPU id and online count (for example from a per-CPU
/// register and a boot-time count); host tests supply a fake. The magazine layer is otherwise
/// identical in both.
pub trait Cpu {
    /// The id of the CPU executing the call.
    fn current(&self) -> usize;
    /// The number of online CPUs.
    fn online(&self) -> usize;
}

/// A fixed-capacity stack of object addresses.
#[derive(Clone, Copy, Debug)]
struct Magazine {
    items: [u64; ROUNDS_MAX],
    len: usize,
    capacity: usize,
}

impl Magazine {
    fn new(capacity: usize) -> Self {
        Self {
            items: [0; ROUNDS_MAX],
            len: 0,
            capacity,
        }
    }

    fn push(&mut self, addr: u64) -> bool {
        if self.len == self.capacity {
            return false;
        }
        self.items[self.len] = addr;
        self.len += 1;
        true
    }

    fn pop(&mut self) -> Option<u64> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(self.items[self.len])
    }

    fn len(&self) -> usize {
        self.len
    }
}

/// One CPU's magazine pair plus its bounded remote free queue.
#[derive(Clone, Copy, Debug)]
struct CpuMag {
    loaded: Magazine,
    prev: Magazine,
    remote: [u64; ROUNDS_MAX],
    remote_len: usize,
}

impl CpuMag {
    fn new(rounds: usize) -> Self {
        Self {
            loaded: Magazine::new(rounds),
            prev: Magazine::new(rounds),
            remote: [0; ROUNDS_MAX],
            remote_len: 0,
        }
    }
}

/// Snapshot of a [`MagCache`]'s counters and accounting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MagCacheStats {
    /// Objects handed out by [`MagCache::alloc`].
    pub allocs: u64,
    /// Objects returned by [`MagCache::free`].
    pub frees: u64,
    /// Objects returned by [`MagCache::remote_free`].
    pub remote_frees: u64,
    /// Full magazines parked in the depot.
    pub depot_full: usize,
    /// Empty magazine shells parked in the depot.
    pub depot_empty: usize,
    /// Times a magazine was refilled from the slab layer.
    pub depot_refills: u64,
    /// Times a full magazine was drained back to the slab layer.
    pub depot_drains: u64,
    /// Magazine capacity in objects.
    pub rounds: usize,
    /// Online CPU count.
    pub online: usize,
    /// Slab-level counters (see [`CacheStats`]).
    pub slab_slabs: usize,
    /// Objects currently removed from the slab layer.
    pub slab_in_use: usize,
    /// Objects still free inside slabs.
    pub slab_free: usize,
    /// Free objects held in loaded/previous magazines.
    pub free_in_mags: usize,
    /// Free objects held in depot magazines.
    pub free_in_depot: usize,
    /// Free objects held in remote queues.
    pub free_in_remote: usize,
}

/// A fixed-size object cache with per-CPU magazines and a shared depot.
///
/// The object data lives in the slab layer (a [`Cache`]); the magazines and depot are staging areas
/// that keep the common case off any shared structure. Construct with the number of online CPUs;
/// every [`MagCache::alloc`] / [`MagCache::free`] / [`MagCache::remote_free`] call then names the
/// current CPU through the [`Cpu`] trait.
pub struct MagCache<const MAX_SLABS: usize, const NCPUS: usize, const NDEPOT: usize> {
    name: &'static str,
    cache: Cache<MAX_SLABS>,
    rounds: usize,
    online: usize,
    depot_full: [Magazine; NDEPOT],
    depot_full_len: usize,
    depot_empty: [Magazine; NDEPOT],
    depot_empty_len: usize,
    cpus: [CpuMag; NCPUS],
    allocs: u64,
    frees: u64,
    remote_frees: u64,
    depot_refills: u64,
    depot_drains: u64,
}

impl<const MAX_SLABS: usize, const NCPUS: usize, const NDEPOT: usize>
    MagCache<MAX_SLABS, NCPUS, NDEPOT>
{
    /// Create a cache for `object_size`-byte objects aligned to `align`, sized for `online_cpus`
    /// CPUs. `object_size` must be non-zero and `align` a power of two (as in [`Cache`]).
    pub fn new(name: &'static str, object_size: usize, align: usize, online_cpus: usize) -> Self {
        let online = online_cpus.max(1).min(NCPUS);
        let rounds = magazine_rounds(online);
        Self {
            name,
            cache: Cache::new(name, object_size, align),
            rounds,
            online,
            depot_full: [Magazine::new(rounds); NDEPOT],
            depot_full_len: 0,
            depot_empty: [Magazine::new(rounds); NDEPOT],
            depot_empty_len: 0,
            cpus: [CpuMag::new(rounds); NCPUS],
            allocs: 0,
            frees: 0,
            remote_frees: 0,
            depot_refills: 0,
            depot_drains: 0,
        }
    }

    /// Allocate one object for the CPU named by `cpu`, or `None` when the source arena is exhausted.
    ///
    /// The fast path pops the current CPU's `loaded` magazine (falling back to `previous`), which
    /// touches no shared state. The slow path drains the CPU's remote queue, then refills from the
    /// depot, then builds a fresh magazine from the slab layer.
    pub fn alloc(&mut self, arena: &mut impl Arena, cpu: &dyn Cpu) -> Option<u64> {
        let id = cpu.current();
        debug_assert!(id < self.online, "cpu id {id} out of range");
        let popped = {
            let mag = &mut self.cpus[id];
            mag.loaded.pop().or_else(|| {
                core::mem::swap(&mut mag.loaded, &mut mag.prev);
                mag.loaded.pop()
            })
        };
        if let Some(addr) = popped {
            self.allocs = self.allocs.saturating_add(1);
            return Some(addr);
        }
        if self.refill(arena, id) {
            let addr = self.cpus[id]
                .loaded
                .pop()
                .expect("refilled magazine is non-empty");
            self.allocs = self.allocs.saturating_add(1);
            return Some(addr);
        }
        None
    }

    /// Return one object allocated by this CPU.
    ///
    /// The fast path pushes to the current CPU's `loaded` magazine. When `loaded` and `previous`
    /// are both full the cache deposits `loaded` into the depot and takes an empty shell back.
    pub fn free(&mut self, cpu: &dyn Cpu, addr: u64) {
        let id = cpu.current();
        debug_assert!(id < self.online, "cpu id {id} out of range");
        {
            let mag = &mut self.cpus[id];
            if mag.loaded.push(addr) {
                self.frees = self.frees.saturating_add(1);
                return;
            }
            core::mem::swap(&mut mag.loaded, &mut mag.prev);
            if mag.loaded.push(addr) {
                self.frees = self.frees.saturating_add(1);
                return;
            }
        }
        // Both magazines are full: deposit one and retry with a fresh shell.
        let empty = self.pop_empty_magazine();
        let full = core::mem::replace(&mut self.cpus[id].loaded, empty);
        self.push_full_magazine(full);
        let pushed = self.cpus[id].loaded.push(addr);
        debug_assert!(pushed);
        self.frees = self.frees.saturating_add(1);
        self.drain_depot_excess();
    }

    /// Return one object whose home CPU is not the freeing CPU.
    ///
    /// The object goes into the freeing CPU's bounded remote queue; when that queue fills, it is
    /// moved into a full magazine in the depot, so the path stays bounded.
    pub fn remote_free(&mut self, cpu: &dyn Cpu, addr: u64) {
        let id = cpu.current();
        debug_assert!(id < self.online, "cpu id {id} out of range");
        let mag = &mut self.cpus[id];
        mag.remote[mag.remote_len] = addr;
        mag.remote_len += 1;
        self.remote_frees = self.remote_frees.saturating_add(1);
        if mag.remote_len == self.rounds {
            let mut full = Magazine::new(self.rounds);
            for i in 0..mag.remote_len {
                full.push(mag.remote[i]);
            }
            mag.remote_len = 0;
            self.push_full_magazine(full);
            self.drain_depot_excess();
        }
    }

    /// Return every object in the magazines, depot and remote queues to the slab layer. Fully-free
    /// slabs are *not* returned to the arena (use [`MagCache::reap`] for that).
    pub fn drain_all(&mut self) {
        for mag in &mut self.cpus[..self.online] {
            while let Some(addr) = mag.loaded.pop() {
                let _ = self.cache.free(addr);
            }
            while let Some(addr) = mag.prev.pop() {
                let _ = self.cache.free(addr);
            }
            for i in 0..mag.remote_len {
                let _ = self.cache.free(mag.remote[i]);
            }
            mag.remote_len = 0;
        }
        for i in 0..self.depot_full_len {
            let full = &mut self.depot_full[i];
            while let Some(addr) = full.pop() {
                let _ = self.cache.free(addr);
            }
        }
        self.depot_full_len = 0;
        self.depot_empty_len = 0;
    }

    /// Drain everything back to the slab layer and return every fully-free slab to the arena.
    /// Returns the number of slabs reaped.
    pub fn reap(&mut self, arena: &mut impl Arena) -> usize {
        self.drain_all();
        self.cache.reap(arena)
    }

    /// Snapshot the cache's counters and accounting.
    pub fn stats(&self) -> MagCacheStats {
        let slab = self.cache.stats();
        let free_in_mags: usize = self.cpus[..self.online]
            .iter()
            .map(|mag| mag.loaded.len() + mag.prev.len())
            .sum();
        let free_in_depot: usize = self.depot_full[..self.depot_full_len]
            .iter()
            .map(Magazine::len)
            .sum();
        let free_in_remote: usize = self.cpus[..self.online]
            .iter()
            .map(|mag| mag.remote_len)
            .sum();
        MagCacheStats {
            allocs: self.allocs,
            frees: self.frees,
            remote_frees: self.remote_frees,
            depot_full: self.depot_full_len,
            depot_empty: self.depot_empty_len,
            depot_refills: self.depot_refills,
            depot_drains: self.depot_drains,
            rounds: self.rounds,
            online: self.online,
            slab_slabs: slab.slabs,
            slab_in_use: slab.in_use,
            slab_free: slab.free,
            free_in_mags,
            free_in_depot,
            free_in_remote,
        }
    }

    /// The slab layer's name, for diagnostics.
    pub fn name(&self) -> &'static str {
        self.name
    }

    fn pop_empty_magazine(&mut self) -> Magazine {
        if self.depot_empty_len > 0 {
            self.depot_empty_len -= 1;
            self.depot_empty[self.depot_empty_len]
        } else {
            Magazine::new(self.rounds)
        }
    }

    fn push_full_magazine(&mut self, full: Magazine) {
        debug_assert!(self.depot_full_len < NDEPOT);
        self.depot_full[self.depot_full_len] = full;
        self.depot_full_len += 1;
    }

    /// Refill the CPU's `loaded` magazine from the remote queue, the depot, or the slab layer.
    /// Returns `false` when the slab layer is exhausted.
    fn refill(&mut self, arena: &mut impl Arena, id: usize) -> bool {
        // Remote queue first, so cross-CPU frees come back promptly.
        if self.cpus[id].remote_len > 0 {
            let mut fresh = Magazine::new(self.rounds);
            for i in 0..self.cpus[id].remote_len {
                fresh.push(self.cpus[id].remote[i]);
            }
            self.cpus[id].remote_len = 0;
            self.cpus[id].loaded = fresh;
            return true;
        }
        if self.depot_full_len > 0 {
            self.depot_full_len -= 1;
            let full = self.depot_full[self.depot_full_len];
            let empty = core::mem::replace(&mut self.cpus[id].loaded, full);
            if self.depot_empty_len < NDEPOT {
                self.depot_empty[self.depot_empty_len] = empty;
                self.depot_empty_len += 1;
            }
            self.depot_refills = self.depot_refills.saturating_add(1);
            return true;
        }
        let mut fresh = Magazine::new(self.rounds);
        for _ in 0..self.rounds {
            match self.cache.alloc(arena) {
                Ok(addr) => {
                    fresh.push(addr);
                }
                Err(KmemError::Exhausted) | Err(_) => break,
            }
        }
        if fresh.len() == 0 {
            return false;
        }
        self.cpus[id].loaded = fresh;
        self.depot_refills = self.depot_refills.saturating_add(1);
        true
    }

    /// Keep the depot bounded: drain full magazines back to the slab layer beyond the bound.
    fn drain_depot_excess(&mut self) {
        let bound = (DEPOT_MULT * self.online).max(2);
        while self.depot_full_len > bound {
            self.depot_full_len -= 1;
            let mut full = self.depot_full[self.depot_full_len];
            while let Some(addr) = full.pop() {
                let _ = self.cache.free(addr);
            }
            self.depot_drains = self.depot_drains.saturating_add(1);
        }
    }
}

#[cfg(test)]
#[path = "magazine_ut.rs"]
mod tests;
