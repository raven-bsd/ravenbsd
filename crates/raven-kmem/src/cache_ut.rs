// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for a single object cache.

use super::*;
use raven_vmem::Vmem;

fn arena(bytes: u64) -> Vmem {
    Vmem::new(0x1000_0000, bytes).expect("arena")
}

#[test]
fn alloc_returns_aligned_distinct_objects() {
    let mut arena = arena(0x4000);
    let mut cache = Cache::<64>::new("t", 100, 16);
    let stride = cache.stride;
    assert_eq!(stride, 112, "100 rounded up to 16");

    let mut seen = alloc::vec::Vec::new();
    for _ in 0..cache.objects_per_slab {
        let ptr = cache.alloc(&mut arena).expect("alloc");
        assert_eq!(ptr % 16, 0, "object is 16-aligned");
        assert!(!seen.contains(&ptr), "objects are distinct");
        seen.push(ptr);
    }
    // One slab covers every object.
    assert_eq!(cache.stats().slabs, 1);
    assert_eq!(cache.stats().in_use, cache.objects_per_slab);
    assert_eq!(cache.stats().free, 0);
}

#[test]
fn free_reuses_the_same_slot() {
    let mut arena = arena(0x4000);
    let mut cache = Cache::<64>::new("t", 32, 8);
    let first = cache.alloc(&mut arena).expect("alloc");
    cache.free(first).expect("free");
    assert_eq!(cache.stats().in_use, 0);
    let again = cache.alloc(&mut arena).expect("alloc");
    assert_eq!(again, first, "freed slot is reused");
}

#[test]
fn growth_allocates_a_second_slab() {
    let mut arena = arena(0x20_0000);
    let mut cache = Cache::<64>::new("t", 64, 8);
    let per_slab = cache.objects_per_slab;
    for _ in 0..(per_slab + 3) {
        cache.alloc(&mut arena).expect("alloc");
    }
    assert_eq!(cache.stats().slabs, 2);
    assert_eq!(cache.stats().in_use, per_slab + 3);
}

#[test]
fn double_free_and_unknown_pointer_are_rejected() {
    let mut arena = arena(0x4000);
    let mut cache = Cache::<64>::new("t", 32, 8);
    let a = cache.alloc(&mut arena).expect("alloc");
    let _b = cache.alloc(&mut arena).expect("alloc");
    cache.free(a).expect("free");
    assert_eq!(cache.free(a), Err(KmemError::DoubleFree));
    assert_eq!(cache.free(a + 1), Err(KmemError::UnknownPointer));
    assert_eq!(cache.free(0), Err(KmemError::UnknownPointer));
}

#[test]
fn reap_returns_fully_free_slabs_to_the_arena() {
    let mut arena = arena(0x20_0000);
    let mut cache = Cache::<64>::new("t", 64, 8);
    let per_slab = cache.objects_per_slab;
    let mut objects = alloc::vec::Vec::new();
    for _ in 0..(per_slab * 2) {
        objects.push(cache.alloc(&mut arena).expect("alloc"));
    }
    assert_eq!(cache.stats().slabs, 2);

    // Free the first slab's objects only; the second slab keeps one object.
    for &ptr in &objects[..per_slab] {
        cache.free(ptr).expect("free");
    }
    for &ptr in &objects[per_slab + 1..] {
        cache.free(ptr).expect("free");
    }
    let reaped = cache.reap(&mut arena);
    assert_eq!(reaped, 1, "only the fully-free slab is reaped");
    assert_eq!(cache.stats().slabs, 1);
    assert_eq!(cache.stats().reaped, 1);
    assert_eq!(cache.stats().in_use, 1);

    // Free the last object; now the slab is reapable.
    cache.free(objects[per_slab]).expect("free");
    assert_eq!(cache.reap(&mut arena), 1);
    assert_eq!(cache.stats().slabs, 0);
}

#[test]
fn release_returns_every_slab() {
    let mut arena = arena(0x20_0000);
    let total_before = arena.total_free();
    let mut cache = Cache::<64>::new("t", 256, 8);
    for _ in 0..(cache.objects_per_slab * 2 + 1) {
        cache.alloc(&mut arena).expect("alloc");
    }
    assert!(cache.stats().slabs >= 2);
    cache.release(&mut arena);
    assert_eq!(cache.stats().slabs, 0);
    assert_eq!(
        arena.total_free(),
        total_before,
        "slabs return to the arena"
    );
}

/// Test-local xorshift generator: deterministic and dependency-free.
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

/// The slab free list invariant: `free_count` is exactly the number of set bits in the slab's
/// free bitmap, no set bit lies outside the slab's object count, and the cache's live objects
/// equal the caller's live set.
fn assert_slab_invariants(cache: &Cache<256>, live: usize) {
    let full_words = cache.objects_per_slab / 64;
    let remainder = cache.objects_per_slab % 64;
    let mut mask = [u64::MAX; FREE_WORDS];
    if remainder != 0 && full_words < FREE_WORDS {
        mask[full_words] = (1u64 << remainder) - 1;
    }
    for word in mask
        .iter_mut()
        .skip(full_words + usize::from(remainder != 0))
    {
        *word = 0;
    }

    let mut in_use = 0usize;
    for slab in &cache.slabs[..cache.slab_count] {
        let bits: usize = slab
            .free_bits
            .iter()
            .map(|word| word.count_ones() as usize)
            .sum();
        assert_eq!(
            slab.free_count, bits,
            "slab at {:#x} free_count diverged from its free bitmap",
            slab.base
        );
        for (word, valid) in slab.free_bits.iter().zip(mask.iter()) {
            assert_eq!(
                word & !valid,
                0,
                "free bit outside the slab's {} objects",
                cache.objects_per_slab
            );
        }
        in_use += cache.objects_per_slab - slab.free_count;
    }
    assert_eq!(in_use, live, "cache live objects disagree with the caller");
}

/// Drive one class cache with the alloc/free mix the kernel heap sees during thread churn.
///
/// The kernel hands out and returns thread, name and cache objects of the same size classes in
/// a LIFO-biased random order. The free list must keep its bitmap and counter in agreement after
/// every operation; a divergence here is the state that reaches the `unreachable!` in `alloc`.
fn churn_class(object_size: usize, align: usize, ops: usize, arena_bytes: u64) {
    let mut arena = arena(arena_bytes);
    let mut cache = Cache::<256>::new("kmem-churn", object_size, align);
    let mut rng = Rng(0x5a1_0c0f_fee1_2345 ^ object_size as u64);
    let mut live: alloc::vec::Vec<u64> = alloc::vec::Vec::new();
    let mut live_set = alloc::collections::BTreeSet::new();

    for _ in 0..ops {
        if live.is_empty() || rng.below(100) < 60 {
            if let Ok(addr) = cache.alloc(&mut arena) {
                assert!(live_set.insert(addr), "address handed out twice");
                live.push(addr);
            }
        } else {
            let index = rng.below(live.len() as u64) as usize;
            let addr = live.swap_remove(index);
            assert!(live_set.remove(&addr), "live set and vector disagree");
            cache.free(addr).expect("freed live object");
        }
        assert_slab_invariants(&cache, live.len());
    }

    for addr in live {
        cache.free(addr).expect("drain");
    }
    assert_eq!(cache.stats().in_use, 0);
    assert_slab_invariants(&cache, 0);
}

#[test]
fn churn_mix_keeps_slab_accounting_balanced() {
    // The size classes the kernel thread churn stresses, plus the smallest class.
    churn_class(192, 16, 20_000, 0x40_0000);
    churn_class(1024, 16, 20_000, 0x40_0000);
    churn_class(1536, 16, 20_000, 0x40_0000);
    churn_class(3072, 16, 20_000, 0x40_0000);
    churn_class(8, 8, 20_000, 0x40_0000);
}
