// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the magazine/depot layer.

use super::*;
use alloc::collections::BTreeSet;
use raven_vmem::Vmem;

/// A fake CPU context for the host tests.
struct IdCpu {
    id: usize,
    online: usize,
}

impl Cpu for IdCpu {
    fn current(&self) -> usize {
        self.id
    }

    fn online(&self) -> usize {
        self.online
    }
}

fn arena(bytes: u64) -> Vmem {
    Vmem::with_capacity(0x1000_0000, bytes, 8192).expect("arena")
}

/// The conservation invariant: every object removed from a slab is either handed out (live), in a
/// magazine, in the depot or in a remote queue — none is lost or duplicated.
fn assert_conserved(cache: &MagCache, live: usize) {
    let s = cache.stats();
    assert_eq!(
        s.slab_in_use,
        live + s.free_in_mags + s.free_in_depot + s.free_in_remote,
        "object conservation violated: in_use={} live={live} mags={} depot={} remote={}",
        s.slab_in_use,
        s.free_in_mags,
        s.free_in_depot,
        s.free_in_remote,
    );
}

#[test]
fn magazine_rounds_scales_with_cpu_count() {
    assert_eq!(magazine_rounds(0), ROUNDS_MIN);
    assert_eq!(magazine_rounds(1), 15);
    assert_eq!(magazine_rounds(2), 30);
    assert_eq!(magazine_rounds(4), 60);
    assert_eq!(magazine_rounds(8), 120);
    assert_eq!(magazine_rounds(9), 135);
    assert_eq!(magazine_rounds(10), ROUNDS_MAX);
    assert_eq!(magazine_rounds(16), ROUNDS_MAX);
    assert_eq!(magazine_rounds(64), ROUNDS_MAX);
}

#[test]
fn fast_path_reuses_freed_objects() {
    let mut arena = arena(0x10_0000);
    let mut cache = MagCache::new("t", 32, 8, 1);
    let cpu = IdCpu { id: 0, online: 1 };
    let mut addrs = alloc::vec::Vec::new();
    for _ in 0..magazine_rounds(1) {
        addrs.push(cache.alloc(&mut arena, &cpu).expect("alloc"));
    }
    for &addr in &addrs {
        cache.free(&cpu, addr);
    }
    let again = cache.alloc(&mut arena, &cpu).expect("reuse");
    assert!(
        addrs.contains(&again),
        "freed object is reused from the magazine"
    );
    assert_conserved(&cache, 1);
}

#[test]
fn depot_deposits_when_both_magazines_are_full() {
    let mut arena = arena(0x40_0000);
    let mut cache = MagCache::new("t", 32, 8, 1);
    let cpu = IdCpu { id: 0, online: 1 };
    let rounds = magazine_rounds(1);
    let count = rounds * 2 + 1;
    let mut addrs = alloc::vec::Vec::new();
    for _ in 0..count {
        addrs.push(cache.alloc(&mut arena, &cpu).expect("alloc"));
    }
    for &addr in &addrs {
        cache.free(&cpu, addr);
    }
    assert!(
        cache.stats().depot_full >= 1,
        "over the two-magazine capacity a magazine is deposited"
    );
    // Every object is still reusable.
    let mut again = alloc::vec::Vec::new();
    for _ in 0..count {
        again.push(cache.alloc(&mut arena, &cpu).expect("reuse"));
    }
    assert_eq!(again.len(), count);
    assert_conserved(&cache, count);
}

#[test]
fn magazine_exhaustion_returns_none_not_panic() {
    // Exactly one 4096-byte slab -> 128 objects of 32 bytes.
    let mut arena = Vmem::new(0x1000_0000, 4096).expect("arena");
    let mut cache = MagCache::new("t", 32, 8, 1);
    let cpu = IdCpu { id: 0, online: 1 };
    let mut count = 0;
    while cache.alloc(&mut arena, &cpu).is_some() {
        count += 1;
    }
    assert_eq!(count, 128, "one slab holds 128 objects");
    assert_eq!(cache.alloc(&mut arena, &cpu), None);
}

#[test]
fn remote_free_returns_to_the_allocating_pool() {
    let mut arena = arena(0x10_0000);
    let mut cache = MagCache::new("t", 16, 8, 2);
    let cpu0 = IdCpu { id: 0, online: 2 };
    let cpu1 = IdCpu { id: 1, online: 2 };
    let addr = cache.alloc(&mut arena, &cpu0).expect("alloc on cpu0");
    cache.remote_free(&cpu1, addr);
    // cpu1's first alloc drains its remote queue before touching the depot.
    assert_eq!(cache.alloc(&mut arena, &cpu1), Some(addr));
}

#[test]
fn remote_free_queue_is_bounded() {
    let mut arena = arena(0x10_0000);
    let mut cache = MagCache::new("t", 16, 8, 1);
    let cpu = IdCpu { id: 0, online: 1 };
    let rounds = magazine_rounds(1);
    let mut addrs = alloc::vec::Vec::new();
    for _ in 0..rounds {
        addrs.push(cache.alloc(&mut arena, &cpu).expect("alloc"));
    }
    for addr in addrs {
        cache.remote_free(&cpu, addr);
    }
    let stats = cache.stats();
    assert_eq!(
        stats.free_in_remote, 0,
        "a full remote queue drains into the depot"
    );
    assert_eq!(stats.free_in_depot, rounds);
    assert_eq!(stats.depot_full, 1);
    assert_conserved(&cache, 0);
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

fn run_hammer(online: usize, ops: usize, arena_bytes: u64) {
    let mut arena = arena(arena_bytes);
    let mut cache = MagCache::new("t", 32, 8, online);
    let mut rng = Rng(0x0ddc_0ffe_e123_4567 ^ online as u64);
    let mut live: Vec<u64> = Vec::new();
    let mut live_set: BTreeSet<u64> = BTreeSet::new();

    for step in 0..ops {
        let cpu = IdCpu {
            id: rng.below(online as u64) as usize,
            online,
        };
        if live.is_empty() || rng.below(100) < 60 {
            if let Some(addr) = cache.alloc(&mut arena, &cpu) {
                assert!(live_set.insert(addr), "address handed out twice");
                live.push(addr);
            }
        } else {
            let idx = rng.below(live.len() as u64) as usize;
            let addr = live.swap_remove(idx);
            assert!(live_set.remove(&addr), "live set and vector disagree");
            if rng.below(100) < 20 {
                cache.remote_free(&cpu, addr);
            } else {
                cache.free(&cpu, addr);
            }
        }
        if step % 5000 == 0 {
            assert_conserved(&cache, live.len());
        }
    }

    let cpu0 = IdCpu { id: 0, online };
    for addr in live.drain(..) {
        cache.free(&cpu0, addr);
    }
    cache.drain_all();
    assert_eq!(cache.stats().slab_in_use, 0, "no object lost or duplicated");
    assert_conserved(&cache, 0);
}

#[test]
fn hammer_one_cpu() {
    run_hammer(1, 50_000, 0x40_0000);
}

#[test]
fn hammer_two_cpus() {
    run_hammer(2, 50_000, 0x40_0000);
}

#[test]
fn hammer_four_cpus() {
    run_hammer(4, 50_000, 0x40_0000);
}

#[test]
fn million_operations_no_lost_or_duplicated() {
    run_hammer(4, 1_000_000, 0x80_0000);
}
