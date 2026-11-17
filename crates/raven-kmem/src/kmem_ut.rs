// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the allocator: general classes, named caches and the hammer.

use super::*;
use alloc::collections::BTreeSet;
use raven_vmem::Vmem;

fn kmem(bytes: u64) -> Kmem {
    Kmem::new(Vmem::with_capacity(0x1000_0000, bytes, 256).expect("arena")).expect("kmem")
}

#[test]
fn general_alloc_and_free_round_trip() {
    let mut kmem = kmem(0x20_0000);
    let a = kmem.alloc(64, 8).expect("alloc");
    let b = kmem.alloc(64, 8).expect("alloc");
    assert_ne!(a, b);
    assert_eq!(a % 8, 0);
    kmem.free(a, 64, 8).expect("free");
    kmem.free(b, 64, 8).expect("free");
    let stats = kmem.stats();
    assert_eq!(stats.allocs, 2);
    assert_eq!(stats.frees, 2);
    assert_eq!(stats.in_use, 0);
}

#[test]
fn class_and_alignment_matrix() {
    let sizes = [
        1usize, 8, 9, 16, 17, 24, 32, 48, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536, 2048,
        3072, 4096, 8192, 8193, 16384,
    ];
    let aligns = [1usize, 8, 16, 32, 64];
    for &size in &sizes {
        for &align in &aligns {
            let mut kmem = kmem(0x40_0000);
            let addr = kmem.alloc(size, align).expect("alloc in a large arena");
            assert_eq!(addr % align as u64, 0, "size {size} align {align}");
            kmem.free(addr, size, align).expect("free round-trips");
        }
    }
}

#[test]
fn large_allocations_go_straight_to_the_arena_and_coalesce() {
    let mut kmem = Kmem::new(Vmem::new(0x1000, 0x400).expect("arena")).expect("kmem");
    // align 32 exceeds the class ceiling, so both go direct.
    let a = kmem.alloc(0x100, 32).expect("a");
    let b = kmem.alloc(0x100, 32).expect("b");
    assert_eq!(kmem.stats().large_live, 2);
    kmem.free(a, 0x100, 32).expect("free a");
    kmem.free(b, 0x100, 32).expect("free b");
    assert_eq!(kmem.stats().large_live, 0);
    assert_eq!(kmem.arena().span_count(), 1);
    assert_eq!(kmem.arena().largest_free(), 0x400);
}

#[test]
fn exhaustion_is_an_error_not_a_panic() {
    let mut kmem = Kmem::new(Vmem::new(0x1000, 0x1000).expect("arena")).expect("kmem");
    assert_eq!(kmem.alloc(0, 8), Err(KmemError::ZeroSize));
    assert_eq!(kmem.alloc(8, 3), Err(KmemError::Alignment));
    assert_eq!(kmem.alloc(8, 0), Err(KmemError::Alignment));
    assert_eq!(kmem.alloc(9000, 8), Err(KmemError::Exhausted));
    // A 4096-byte class slab fits exactly once; the second slab fails.
    let a = kmem.alloc(4096, 8).expect("first slab");
    assert_eq!(kmem.alloc(4096, 8), Err(KmemError::Exhausted));
    assert!(kmem.stats().alloc_fail > 0);
    kmem.free(a, 4096, 8).expect("free");
}

#[test]
fn named_cache_lifecycle() {
    let mut kmem = kmem(0x20_0000);
    let id = kmem.cache_create("thread", 96, 8).expect("create");
    assert!(id.0 >= SIZE_CLASSES.len());
    let a = kmem.cache_alloc(id).expect("alloc a");
    let b = kmem.cache_alloc(id).expect("alloc b");
    assert_ne!(a, b);
    assert_eq!(a % 8, 0);
    let stats = kmem.cache_stats(id).expect("stats");
    assert_eq!(stats.name, "thread");
    assert_eq!(stats.in_use, 2);

    kmem.cache_free(id, a).expect("free a");
    assert_eq!(kmem.cache_alloc(id).expect("reuse"), a, "slot is reused");
    kmem.cache_free(id, a).expect("free a again");
    kmem.cache_free(id, b).expect("free b");
    assert_eq!(kmem.cache_reap(id).expect("reap"), 1);
    kmem.cache_destroy(id).expect("destroy");
    assert_eq!(kmem.cache_alloc(id), Err(KmemError::InvalidCache));
    assert_eq!(kmem.cache_stats(id), None);
}

#[test]
fn cache_handles_and_arguments_are_validated() {
    let mut kmem = kmem(0x20_0000);
    assert_eq!(
        kmem.cache_alloc(CacheId(9999)),
        Err(KmemError::InvalidCache)
    );
    assert_eq!(
        kmem.cache_free(CacheId(9999), 0),
        Err(KmemError::InvalidCache)
    );
    assert_eq!(kmem.cache_reap(CacheId(9999)), Err(KmemError::InvalidCache));
    assert_eq!(kmem.cache_stats(CacheId(9999)), None);
    assert_eq!(kmem.cache_create("z", 0, 8), Err(KmemError::ZeroSize));
    assert_eq!(kmem.cache_create("z", 8, 3), Err(KmemError::Alignment));
    // The general size classes cannot be destroyed.
    assert_eq!(kmem.cache_destroy(CacheId(0)), Err(KmemError::InvalidCache));
}

#[test]
fn destroyed_slot_is_reused() {
    let mut kmem = kmem(0x20_0000);
    let id1 = kmem.cache_create("a", 8, 8).expect("create a");
    kmem.cache_destroy(id1).expect("destroy a");
    let id2 = kmem.cache_create("b", 16, 8).expect("create b");
    assert_eq!(id1, id2, "a destroyed slot is reused");
    assert_eq!(kmem.cache_stats(id2).expect("stats").name, "b");
}

#[test]
fn stats_aggregate_across_caches() {
    let mut kmem = kmem(0x20_0000);
    let initial = kmem.stats();
    assert_eq!(initial.caches, SIZE_CLASSES.len());
    let a = kmem.alloc(64, 8).expect("alloc");
    assert_eq!(kmem.stats().allocs, 1);
    assert_eq!(kmem.stats().in_use, 1);
    kmem.free(a, 64, 8).expect("free");
    let final_ = kmem.stats();
    assert_eq!(final_.frees, 1);
    assert_eq!(final_.in_use, 0);
    assert_eq!(final_.alloc_fail, 0);
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

#[test]
fn deterministic_hammer() {
    const ARENA_BYTES: u64 = 0x80_0000;
    let mut kmem = Kmem::new(Vmem::with_capacity(0x1000_0000, ARENA_BYTES, 8192).expect("arena"))
        .expect("kmem");
    let mut rng = Rng(0x1234_5678_90ab_cdef);
    let mut live: Vec<(u64, usize, usize)> = Vec::new();
    let mut live_addrs: BTreeSet<u64> = BTreeSet::new();

    for _ in 0..3000 {
        if live.is_empty() || rng.below(100) < 55 {
            let size = 1 + rng.below(9000) as usize;
            let align = 1usize << rng.below(13);
            if let Ok(addr) = kmem.alloc(size, align) {
                assert_eq!(addr % align as u64, 0);
                assert!(live_addrs.insert(addr), "address handed out twice");
                live.push((addr, size, align));
            }
        } else {
            let idx = rng.below(live.len() as u64) as usize;
            let (addr, size, align) = live.swap_remove(idx);
            kmem.free(addr, size, align).expect("free live block");
            live_addrs.remove(&addr);
        }
    }
    for (addr, size, align) in live.drain(..) {
        kmem.free(addr, size, align).expect("free remaining");
    }

    let stats = kmem.stats();
    assert_eq!(stats.frees, stats.allocs);
    assert_eq!(stats.in_use, 0);
    assert_eq!(stats.large_live, 0);

    // Reap every cache (the general classes included) to prove full coalescing.
    for index in 0..SIZE_CLASSES.len() {
        kmem.cache_reap(CacheId(index)).expect("reap general class");
    }
    assert_eq!(kmem.arena().span_count(), 1);
    assert_eq!(kmem.arena().largest_free(), ARENA_BYTES);
}
