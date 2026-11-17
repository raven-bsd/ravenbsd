// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for a single object cache.

use super::*;
use raven_vmem::Vmem;

fn arena(bytes: u64) -> Vmem {
    Vmem::with_capacity(0x1000_0000, bytes, 256).expect("arena")
}

#[test]
fn alloc_returns_aligned_distinct_objects() {
    let mut arena = arena(0x4000);
    let mut cache = Cache::new("t", 100, 16);
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
    let mut cache = Cache::new("t", 32, 8);
    let first = cache.alloc(&mut arena).expect("alloc");
    cache.free(first).expect("free");
    assert_eq!(cache.stats().in_use, 0);
    let again = cache.alloc(&mut arena).expect("alloc");
    assert_eq!(again, first, "freed slot is reused");
}

#[test]
fn growth_allocates_a_second_slab() {
    let mut arena = arena(0x20_0000);
    let mut cache = Cache::new("t", 64, 8);
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
    let mut cache = Cache::new("t", 32, 8);
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
    let mut cache = Cache::new("t", 64, 8);
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
    let mut cache = Cache::new("t", 256, 8);
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
