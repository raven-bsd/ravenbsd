// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the per-CPU frame magazine layer.

use super::*;
use crate::buddy::{BuddyAllocator, BuddyError, NoPager, Pager};
use std::collections::BTreeSet;

/// 128-frame buddy: orders 0..=7.
type Buddy = BuddyAllocator<8, 128, NoPager>;
/// 4 CPUs, 32-frame caches, 16-frame remote queues.
type Cache = FrameCache<4, 32, 16>;

/// A pager whose `back` always fails.
struct FailingPager;

impl Pager for FailingPager {
    fn back(&mut self, _frame: u64, _count: u64) -> bool {
        false
    }

    fn unback(&mut self, _frame: u64, _count: u64) {}
}

/// `free (buddy) + cached + remote + live + reserved == total`.
fn assert_conserved(buddy: &Buddy, cache: &Cache, live: &BTreeSet<u64>) {
    let stats = buddy.stats();
    assert_eq!(
        stats.free
            + cache.cached_total() as u64
            + cache.remote_total() as u64
            + live.len() as u64
            + stats.reserved,
        stats.total,
        "frame conservation violated"
    );
    // No cache is above its capacity.
    for cpu in 0..cache.online() {
        assert!(cache.cached(cpu) <= 32);
        assert!(cache.remote_frames(cpu) <= 16);
    }
}

#[test]
fn fast_path_hits_and_refill_stats() {
    let mut buddy = Buddy::new(0, 128).expect("buddy");
    let mut cache = Cache::new(1, 0, 24, 16, 16);
    // First allocation misses and refills a batch; the rest hit the cache.
    let first = cache.alloc(&mut buddy, 0).expect("alloc");
    for _ in 0..15 {
        cache.alloc(&mut buddy, 0).expect("alloc");
    }
    let stats = cache.cpu_stats(0);
    assert_eq!(stats.misses, 1);
    assert_eq!(stats.hits, 15);
    assert_eq!(stats.refills, 1);
    assert_eq!(buddy.free_frames(), 128 - 16);
    let _ = first;
}

#[test]
fn high_watermark_drains_a_batch() {
    let mut buddy = Buddy::new(0, 128).expect("buddy");
    let mut cache = Cache::new(1, 4, 8, 16, 4);
    // Free 12 frames: the cache crosses the high watermark (8) and drains batches.
    let frames: Vec<u64> = (0..12)
        .map(|_| cache.alloc(&mut buddy, 0).expect("alloc"))
        .collect();
    for frame in frames {
        cache.free(&mut buddy, 0, frame);
    }
    let stats = cache.cpu_stats(0);
    assert!(stats.drains >= 1);
    assert!(stats.evictions >= 1);
    // Everything the test freed is back in the buddy or the cache; conserves.
    let live = BTreeSet::new();
    assert_conserved(&buddy, &cache, &live);
}

#[test]
fn remote_free_is_bounded_and_reused() {
    let mut buddy = Buddy::new(0, 128).expect("buddy");
    let mut cache = Cache::new(2, 8, 24, 16, 8);
    let frame = cache.alloc(&mut buddy, 0).expect("alloc");
    cache.remote_free(&mut buddy, 1, frame);
    assert_eq!(cache.remote_frames(1), 1);
    // The owner (cpu 1) reuses the remotely-freed frame before touching the buddy.
    let reused = cache.alloc(&mut buddy, 1).expect("alloc");
    assert_eq!(reused, frame);
    assert_eq!(cache.remote_frames(1), 0);
}

#[test]
fn pager_failure_returns_a_typed_error() {
    let mut buddy =
        BuddyAllocator::<8, 128, FailingPager>::with_pager(0, 128, FailingPager).expect("buddy");
    let mut cache = Cache::new(1, 4, 8, 16, 4);
    assert_eq!(cache.alloc(&mut buddy, 0), Err(BuddyError::Pager));
    assert_eq!(buddy.free_frames(), 128, "nothing leaked");
    assert_eq!(cache.cached_total(), 0);
}

#[test]
fn deterministic_drain_order() {
    let mut buddy = Buddy::new(0, 128).expect("buddy");
    let mut cache = Cache::new(1, 0, 32, 4, 4);
    let a = cache.alloc(&mut buddy, 0).expect("a");
    let b = cache.alloc(&mut buddy, 0).expect("b");
    cache.free(&mut buddy, 0, a);
    cache.free(&mut buddy, 0, b);
    let cached = cache.cached(0);
    // Drain returns every cached frame back to the buddy, in LIFO order.
    let returned = cache.drain(&mut buddy, 0);
    assert_eq!(returned, cached);
    assert_eq!(buddy.free_frames(), 128);
    assert_eq!(cache.cached_total(), 0);
    assert_eq!(cache.remote_total(), 0);
}

#[test]
fn single_cpu_baseline() {
    let mut buddy = Buddy::new(0, 128).expect("buddy");
    let mut cache = Cache::new(1, 8, 24, 16, 8);
    let mut live = BTreeSet::new();
    for _ in 0..64 {
        let frame = cache.alloc(&mut buddy, 0).expect("alloc");
        assert!(live.insert(frame), "single CPU never duplicates");
    }
    // Free half.
    let mut to_free: Vec<u64> = live.iter().copied().take(32).collect();
    for frame in to_free.drain(..) {
        live.remove(&frame);
        cache.free(&mut buddy, 0, frame);
    }
    assert_conserved(&buddy, &cache, &live);
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        let mut value = self.0;
        value ^= value << 13;
        value ^= value >> 7;
        value ^= value << 17;
        self.0 = value;
        value
    }

    fn below(&mut self, limit: u64) -> u64 {
        self.next() % limit
    }
}

fn run_model(cpus: usize, ops: usize) {
    let mut buddy = Buddy::new(0, 128).expect("buddy");
    let mut cache = Cache::new(cpus, 8, 24, 16, 8);
    let mut rng = Rng(0x0ddc_0ffe_e123_4567 ^ cpus as u64);
    let mut live: BTreeSet<u64> = BTreeSet::new();

    for _ in 0..ops {
        let cpu = rng.below(cpus as u64) as usize;
        if live.is_empty() || rng.below(100) < 55 {
            match cache.alloc(&mut buddy, cpu) {
                Ok(frame) => {
                    assert!(live.insert(frame), "frame handed out twice");
                }
                Err(BuddyError::OutOfFrames) => {}
                Err(other) => panic!("unexpected error: {other:?}"),
            }
        } else {
            let idx = rng.below(live.len() as u64) as usize;
            let frame = *live.iter().nth(idx).expect("live frame");
            live.remove(&frame);
            if rng.below(100) < 25 {
                cache.remote_free(&mut buddy, cpu, frame);
            } else {
                cache.free(&mut buddy, cpu, frame);
            }
        }
        if live.len() < 64 {
            assert_conserved(&buddy, &cache, &live);
        }
    }

    // Drain every CPU's cache and remote queue.
    for cpu in 0..cpus {
        cache.drain(&mut buddy, cpu);
    }
    // Free the still-live frames and verify full coalescing.
    for frame in live.iter() {
        buddy.free(*frame, 0).expect("free live");
    }
    assert_eq!(buddy.free_frames(), 128);
    assert_eq!(
        buddy.free_blocks(7),
        1,
        "everything coalesces back to one block"
    );
}

#[test]
fn four_cpu_million_operation_model() {
    run_model(4, 1_000_000);
}

#[test]
fn single_cpu_model() {
    run_model(1, 200_000);
}
