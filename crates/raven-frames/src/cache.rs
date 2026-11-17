// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Per-CPU order-0 frame magazines over the buddy allocator.
//!
//! The buddy allocator (A5a) is the shared source of order-0 frames; this layer adds fixed-capacity
//! per-CPU caches so the common case of a single frame never touches the shared buddy. It is pure:
//! every call takes an explicit CPU id and a [`FrameSource`], reading no hardware or global state.

#![forbid(unsafe_code)]

use crate::buddy::{BuddyAllocator, BuddyError, Pager};

/// The shared source of order-0 frames a magazine layer draws from.
pub trait FrameSource {
    /// Allocate one order-0 frame.
    fn alloc_frame(&mut self) -> Result<u64, BuddyError>;
    /// Free one order-0 frame.
    fn free_frame(&mut self, frame: u64) -> Result<(), BuddyError>;
}

impl<const ORDERS: usize, const MAX_BLOCKS: usize, P: Pager> FrameSource
    for BuddyAllocator<ORDERS, MAX_BLOCKS, P>
{
    fn alloc_frame(&mut self) -> Result<u64, BuddyError> {
        self.alloc(0)
    }

    fn free_frame(&mut self, frame: u64) -> Result<(), BuddyError> {
        self.free(frame, 0)
    }
}

/// Per-CPU magazine statistics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CpuFrameStats {
    /// Allocations served from the cache (no buddy refill).
    pub hits: u64,
    /// Allocations that found the cache empty.
    pub misses: u64,
    /// Batch refills from the buddy.
    pub refills: u64,
    /// Batch drains to the buddy.
    pub drains: u64,
    /// Frames returned through the remote-free queue.
    pub remote_frees: u64,
    /// Frames evicted to the buddy by the high watermark.
    pub evictions: u64,
}

/// One CPU's frame cache and its bounded remote-free queue.
#[derive(Clone, Copy, Debug)]
struct CpuCache<const CACHE_CAP: usize, const REMOTE_CAP: usize> {
    frames: [u64; CACHE_CAP],
    len: usize,
    remote: [u64; REMOTE_CAP],
    remote_len: usize,
    stats: CpuFrameStats,
}

impl<const CACHE_CAP: usize, const REMOTE_CAP: usize> CpuCache<CACHE_CAP, REMOTE_CAP> {
    const fn new() -> Self {
        Self {
            frames: [0; CACHE_CAP],
            len: 0,
            remote: [0; REMOTE_CAP],
            remote_len: 0,
            stats: CpuFrameStats {
                hits: 0,
                misses: 0,
                refills: 0,
                drains: 0,
                remote_frees: 0,
                evictions: 0,
            },
        }
    }

    fn push(&mut self, frame: u64) -> bool {
        if self.len == CACHE_CAP {
            return false;
        }
        self.frames[self.len] = frame;
        self.len += 1;
        true
    }

    fn pop(&mut self) -> Option<u64> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(self.frames[self.len])
    }
}

/// A per-CPU magazine layer over a caller-owned [`FrameSource`].
///
/// `NCPUS` is the maximum number of CPUs, `CACHE_CAP` the frames each CPU's cache holds and
/// `REMOTE_CAP` the bounded remote-free queue per CPU. The watermarks and batch sizes are runtime
/// parameters; all storage is fixed inline, so construction never allocates.
pub struct FrameCache<const NCPUS: usize, const CACHE_CAP: usize, const REMOTE_CAP: usize> {
    cpus: [CpuCache<CACHE_CAP, REMOTE_CAP>; NCPUS],
    online: usize,
    low_watermark: usize,
    high_watermark: usize,
    refill_batch: usize,
    drain_batch: usize,
}

impl<const NCPUS: usize, const CACHE_CAP: usize, const REMOTE_CAP: usize>
    FrameCache<NCPUS, CACHE_CAP, REMOTE_CAP>
{
    /// Create a magazine layer for `online_cpus` CPUs with the given watermarks and batch sizes.
    pub fn new(
        online_cpus: usize,
        low_watermark: usize,
        high_watermark: usize,
        refill_batch: usize,
        drain_batch: usize,
    ) -> Self {
        debug_assert!(high_watermark <= CACHE_CAP);
        Self {
            cpus: [CpuCache::new(); NCPUS],
            online: online_cpus.max(1).min(NCPUS),
            low_watermark,
            high_watermark,
            refill_batch,
            drain_batch,
        }
    }

    /// Number of online CPUs.
    pub fn online(&self) -> usize {
        self.online
    }

    /// Allocate one frame for `cpu`.
    ///
    /// The fast path pops the CPU's cache; on a miss it first reuses remotely-freed frames, then
    /// refills a batch from the source. A pager or OOM failure in the source surfaces as a typed
    /// error and never loses or duplicates a frame.
    pub fn alloc(&mut self, source: &mut impl FrameSource, cpu: usize) -> Result<u64, BuddyError> {
        let hit = {
            let cache = &mut self.cpus[cpu];
            if let Some(frame) = cache.pop() {
                cache.stats.hits = cache.stats.hits.saturating_add(1);
                Some(frame)
            } else {
                cache.stats.misses = cache.stats.misses.saturating_add(1);
                None
            }
        };
        if let Some(frame) = hit {
            if self.cpus[cpu].len < self.low_watermark {
                self.refill(source, cpu);
            }
            return Ok(frame);
        }

        // Reuse remotely-freed frames first.
        if self.cpus[cpu].remote_len > 0 {
            self.cpus[cpu].remote_len -= 1;
            let frame = self.cpus[cpu].remote[self.cpus[cpu].remote_len];
            self.cpus[cpu].stats.hits = self.cpus[cpu].stats.hits.saturating_add(1);
            return Ok(frame);
        }

        // Refill a batch, then pop one.
        let first_error = self.refill(source, cpu);
        if let Some(frame) = self.cpus[cpu].pop() {
            return Ok(frame);
        }
        Err(first_error.unwrap_or(BuddyError::OutOfFrames))
    }

    /// Return one frame allocated by this CPU.
    ///
    /// The fast path pushes to the CPU's cache; when the cache exceeds the high watermark a batch
    /// is drained back to the source.
    pub fn free(&mut self, source: &mut impl FrameSource, cpu: usize, frame: u64) {
        if !self.cpus[cpu].push(frame) {
            // Cache full: drain a batch, then retry.
            self.drain_to_source(source, cpu);
            let pushed = self.cpus[cpu].push(frame);
            debug_assert!(pushed);
        }
        if self.cpus[cpu].len > self.high_watermark {
            self.drain_to_source(source, cpu);
        }
    }

    /// Return a frame whose home CPU is not the freeing CPU.
    ///
    /// The frame lands in the freeing CPU's bounded remote queue; when the queue fills, it is
    /// folded into the cache (or freed to the source), keeping the path bounded.
    pub fn remote_free(&mut self, source: &mut impl FrameSource, cpu: usize, frame: u64) {
        if self.cpus[cpu].remote_len == REMOTE_CAP {
            self.fold_remote(source, cpu);
        }
        self.cpus[cpu].remote[self.cpus[cpu].remote_len] = frame;
        self.cpus[cpu].remote_len += 1;
        self.cpus[cpu].stats.remote_frees = self.cpus[cpu].stats.remote_frees.saturating_add(1);
    }

    /// Deterministically drain a CPU's cache and remote queue back to the source. Returns the
    /// number of frames returned.
    ///
    /// Cached frames are returned LIFO, then remote frames FIFO.
    pub fn drain(&mut self, source: &mut impl FrameSource, cpu: usize) -> usize {
        let mut returned = 0;
        while let Some(frame) = self.cpus[cpu].pop() {
            let _ = source.free_frame(frame);
            returned += 1;
        }
        for i in 0..self.cpus[cpu].remote_len {
            let _ = source.free_frame(self.cpus[cpu].remote[i]);
            returned += 1;
        }
        self.cpus[cpu].remote_len = 0;
        returned
    }

    /// The cache's frames currently cached for `cpu`.
    pub fn cached(&self, cpu: usize) -> usize {
        self.cpus[cpu].len
    }

    /// The cache's remote-free queue length for `cpu`.
    pub fn remote_frames(&self, cpu: usize) -> usize {
        self.cpus[cpu].remote_len
    }

    /// Total frames cached across all CPUs.
    pub fn cached_total(&self) -> usize {
        self.cpus[..self.online].iter().map(|cpu| cpu.len).sum()
    }

    /// Total remote-free frames across all CPUs.
    pub fn remote_total(&self) -> usize {
        self.cpus[..self.online]
            .iter()
            .map(|cpu| cpu.remote_len)
            .sum()
    }

    /// Statistics for one CPU.
    pub fn cpu_stats(&self, cpu: usize) -> CpuFrameStats {
        self.cpus[cpu].stats
    }

    fn refill(&mut self, source: &mut impl FrameSource, cpu: usize) -> Option<BuddyError> {
        let mut first_error = None;
        for _ in 0..self.refill_batch {
            match source.alloc_frame() {
                Ok(frame) => {
                    if !self.cpus[cpu].push(frame) {
                        let _ = source.free_frame(frame);
                        break;
                    }
                }
                Err(error) => {
                    if first_error.is_none() {
                        first_error = Some(error);
                    }
                    break;
                }
            }
        }
        self.cpus[cpu].stats.refills = self.cpus[cpu].stats.refills.saturating_add(1);
        first_error
    }

    fn drain_to_source(&mut self, source: &mut impl FrameSource, cpu: usize) {
        let count = self.drain_batch.min(self.cpus[cpu].len);
        for _ in 0..count {
            let frame = self.cpus[cpu].pop().expect("drain within len");
            let _ = source.free_frame(frame);
            self.cpus[cpu].stats.evictions = self.cpus[cpu].stats.evictions.saturating_add(1);
        }
        self.cpus[cpu].stats.drains = self.cpus[cpu].stats.drains.saturating_add(1);
    }

    fn fold_remote(&mut self, source: &mut impl FrameSource, cpu: usize) {
        let mut i = 0;
        while i < self.cpus[cpu].remote_len {
            let frame = self.cpus[cpu].remote[i];
            if self.cpus[cpu].len < CACHE_CAP {
                self.cpus[cpu].frames[self.cpus[cpu].len] = frame;
                self.cpus[cpu].len += 1;
            } else {
                let _ = source.free_frame(frame);
            }
            i += 1;
        }
        self.cpus[cpu].remote_len = 0;
    }
}

#[cfg(test)]
#[path = "cache_ut.rs"]
mod tests;
