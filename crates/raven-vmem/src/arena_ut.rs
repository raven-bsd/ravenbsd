// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the arena: targeted error paths, exhaustive state machine and property checks.

use super::*;

fn assert_tiles(arena: &Vmem, start: u64, end: u64) {
    let mut cursor = start;
    let mut previous_free = false;
    let mut count = 0usize;
    for segment in arena.segments() {
        assert_eq!(segment.base(), cursor, "gap or overlap in the tag list");
        assert!(segment.size() > 0, "empty tag");
        assert!(
            !(previous_free && segment.is_free()),
            "adjacent free tags were not coalesced"
        );
        previous_free = segment.is_free();
        cursor = segment.end();
        count += 1;
    }
    assert_eq!(cursor, end, "tags do not cover the imported range");
    assert_eq!(arena.span_count(), count);
}

fn assert_no_overlap(live: &[(u64, u64)]) {
    let mut sorted = live.to_vec();
    sorted.sort_unstable();
    for pair in sorted.windows(2) {
        assert!(
            pair[0].0 + pair[0].1 <= pair[1].0,
            "live segments overlap: {pair:?}"
        );
    }
}

fn sorted(mut ranges: Vec<(u64, u64)>) -> Vec<(u64, u64)> {
    ranges.sort_unstable();
    ranges
}

fn check_all(arena: &Vmem, start: u64, end: u64, live: &[(u64, u64)]) {
    assert_tiles(arena, start, end);
    assert_no_overlap(live);
    let allocated: Vec<(u64, u64)> = arena
        .allocated_segments()
        .map(|segment| (segment.base(), segment.size()))
        .collect();
    assert_eq!(sorted(allocated), sorted(live.to_vec()));
    for &(base, size) in live {
        assert!(arena.contains_range(base, size));
    }
    let used: u64 = live.iter().map(|(_, size)| size).sum();
    assert_eq!(arena.total_allocated(), used);
    assert_eq!(
        arena.total_allocated() + arena.total_free(),
        end - start,
        "byte conservation failed"
    );
    assert_eq!(arena.stats().spans, arena.span_count());
    assert_eq!(arena.stats().free_bytes, arena.total_free());
    assert_eq!(arena.stats().used_bytes, arena.total_allocated());
    assert_eq!(arena.stats().largest_free, arena.largest_free());
    let fragmentation = arena.fragmentation();
    assert!((0.0..=1.0).contains(&fragmentation));
}

#[test]
fn new_and_import_cover_the_ranges() {
    let mut arena = Vmem::new(0x1000, 0x100).expect("arena");
    assert!(arena.contains(0x1000));
    assert!(arena.contains(0x10ff));
    assert!(!arena.contains(0x1100));
    assert!(!arena.contains(0x0fff));
    assert!(arena.contains_range(0x1000, 0x100));
    assert!(!arena.contains_range(0x1080, 0x100));
    assert!(!arena.contains_range(0x0fff, 0x2));
    assert_eq!(arena.span_count(), 1);
    assert_eq!(arena.free_span_count(), 1);
    assert_eq!(arena.allocated_span_count(), 0);
    assert_eq!(arena.largest_free(), 0x100);
    assert_eq!(arena.total_free(), 0x100);
    assert_eq!(arena.total_allocated(), 0);
    assert_eq!(arena.fragmentation(), 0.0);

    arena.import(0x2000, 0x100).expect("disjoint import");
    assert_eq!(arena.span_count(), 2);
    assert!(arena.contains(0x2000));

    arena.import(0x1100, 0x100).expect("adjacent import");
    assert_eq!(arena.span_count(), 2, "adjacent free imports coalesce");
    assert_eq!(arena.segments().next().expect("span").size(), 0x200);
    assert_eq!(arena.total_free(), 0x300);
}

#[test]
fn import_errors_are_rejected() {
    let mut arena = Vmem::new(0x1000, 0x100).expect("arena");
    let before = arena.span_count();
    assert_eq!(
        arena.import(0x1080, 0x10),
        Err(VmemError::Overlap),
        "middle overlap"
    );
    assert_eq!(
        arena.import(0x0f80, 0x100),
        Err(VmemError::Overlap),
        "overlap from below"
    );
    assert_eq!(
        arena.import(0x10f0, 0x20),
        Err(VmemError::Overlap),
        "overlap from above"
    );
    assert_eq!(arena.import(0x2000, 0), Err(VmemError::ZeroLength));
    assert_eq!(arena.import(u64::MAX, 2), Err(VmemError::Overflow));
    assert_eq!(arena.span_count(), before);
    assert_eq!(Vmem::new(0, 0).err(), Some(VmemError::ZeroLength));
    assert_eq!(Vmem::new(u64::MAX - 1, 2).err(), Some(VmemError::Overflow));
    assert_eq!(
        Vmem::with_capacity(0, 0x100, 0).err(),
        Some(VmemError::NoTags)
    );
}

#[test]
fn alloc_alignment_splits_and_coalesces() {
    let mut arena = Vmem::new(0x1001, 0x3000).expect("arena");
    let base = arena.alloc(0x10, 0x1000).expect("aligned allocation");
    assert_eq!(base, 0x2000);
    assert_eq!(base & 0xfff, 0);
    assert_eq!(arena.span_count(), 3);
    let segments: Vec<(u64, u64, bool)> = arena
        .segments()
        .map(|segment| (segment.base(), segment.size(), segment.is_free()))
        .collect();
    assert_eq!(
        segments,
        vec![
            (0x1001, 0x0fff, true),
            (0x2000, 0x10, false),
            (0x2010, 0x1ff1, true),
        ]
    );
    assert_eq!(arena.largest_free(), 0x1ff1);
    assert_eq!(arena.total_allocated(), 0x10);
    arena.free(base, 0x10).expect("free");
    assert_eq!(arena.span_count(), 1);
    assert_eq!(arena.largest_free(), 0x3000);
    assert_eq!(arena.fragmentation(), 0.0);
}

#[test]
fn alloc_at_places_and_splits_exactly() {
    let mut arena = Vmem::new(0x1000, 0x200).expect("arena");
    arena
        .alloc_at(0x1040, 0x20, 0x10)
        .expect("aligned placement");
    assert_eq!(
        arena
            .segments()
            .map(|segment| (segment.base(), segment.size(), segment.is_free()))
            .collect::<Vec<_>>(),
        vec![
            (0x1000, 0x40, true),
            (0x1040, 0x20, false),
            (0x1060, 0x1a0, true),
        ]
    );
    arena.alloc_at(0x1000, 0x40, 1).expect("prefix exactly");
    arena
        .alloc_at(0x1080, 0x40, 0x20)
        .expect("aligned in suffix");
    assert!(arena.contains_range(0x1080, 0x40));
    assert_eq!(arena.alloc_at(0x1010, 0x10, 1), Err(VmemError::Overlap));
    assert_eq!(arena.alloc_at(0x11f0, 0x20, 1), Err(VmemError::OutOfRange));
    assert_eq!(arena.alloc_at(0x1050, 0x8, 1), Err(VmemError::Overlap));
    arena
        .alloc_at(0x1100, 0x100, 1)
        .expect("suffix tail exactly");
    arena.free(0x1040, 0x20).expect("first placement");
    arena.free(0x1000, 0x40).expect("prefix");
    arena.free(0x1080, 0x40).expect("suffix placement");
    arena.free(0x1100, 0x100).expect("suffix tail");
    assert_eq!(arena.span_count(), 1);
    assert_eq!(arena.total_free(), 0x200);
}

#[test]
fn allocation_errors_leave_the_arena_untouched() {
    let mut arena = Vmem::new(0x1000, 0x100).expect("arena");
    let before = arena.stats();
    assert_eq!(arena.alloc(0, 1), Err(VmemError::ZeroLength));
    assert_eq!(arena.alloc(1, 0), Err(VmemError::Alignment));
    assert_eq!(arena.alloc(1, 3), Err(VmemError::Alignment));
    assert_eq!(arena.alloc(0x101, 1), Err(VmemError::Exhausted));
    assert_eq!(arena.alloc(u64::MAX, 1), Err(VmemError::Exhausted));
    assert_eq!(
        arena.alloc_at(0x1008, 0x10, 0x10),
        Err(VmemError::Alignment)
    );
    assert_eq!(arena.alloc_at(0x800, 0x10, 1), Err(VmemError::OutOfRange));
    assert_eq!(arena.alloc_at(0x10f8, 0x10, 1), Err(VmemError::OutOfRange));
    assert_eq!(arena.alloc_at(u64::MAX, 2, 1), Err(VmemError::Overflow));
    assert_eq!(arena.alloc_at(0x1000, 0, 1), Err(VmemError::ZeroLength));
    assert_eq!(arena.span_count(), before.spans);
    assert_eq!(arena.total_free(), before.free_bytes);
    assert_eq!(arena.stats().allocs, 0);
    assert_eq!(arena.stats().frees, 0);
    assert_eq!(arena.stats().alloc_fail, 10);
}

#[test]
fn free_errors_are_rejected() {
    let mut arena = Vmem::new(0x1000, 0x100).expect("arena");
    let base = arena.alloc(0x20, 1).expect("allocation");
    assert_eq!(arena.free(0x1010, 0x20), Err(VmemError::UnknownSegment));
    assert_eq!(arena.free(0x1800, 0x20), Err(VmemError::UnknownSegment));
    assert_eq!(arena.free(base, 0x10), Err(VmemError::SizeMismatch));
    assert_eq!(arena.free(base, 0), Err(VmemError::ZeroLength));
    arena.free(base, 0x20).expect("first free");
    assert_eq!(arena.free(base, 0x20), Err(VmemError::DoubleFree));
    assert_eq!(arena.stats().frees, 1);
    assert_eq!(arena.stats().alloc_fail, 0);
}

#[test]
fn first_best_and_next_fit_differ() {
    let mut arena = Vmem::new(0, 0x400).expect("arena");
    assert_eq!(arena.alloc(0x100, 1).expect("a"), 0);
    assert_eq!(arena.alloc(0x80, 1).expect("b"), 0x100);
    arena.free(0, 0x100).expect("free a");
    assert_eq!(
        arena
            .alloc_with(0x40, 1, Policy::FirstFit)
            .expect("first fit"),
        0
    );
    assert_eq!(
        arena
            .alloc_with(0x40, 1, Policy::BestFit)
            .expect("best fit"),
        0x40,
        "best fit prefers the smaller remaining free extent"
    );

    let mut arena = Vmem::new(0, 0x40).expect("arena");
    assert_eq!(
        arena.alloc_with(0x10, 1, Policy::NextFit).expect("next 0"),
        0x00
    );
    assert_eq!(
        arena.alloc_with(0x10, 1, Policy::NextFit).expect("next 1"),
        0x10
    );
    arena.free(0, 0x10).expect("free low");
    assert_eq!(
        arena
            .alloc_with(0x10, 1, Policy::NextFit)
            .expect("skip low"),
        0x20,
        "next fit does not reuse the lowest free address"
    );
    arena.free(0x10, 0x10).expect("free middle");
    arena.free(0x20, 0x10).expect("free high");
    assert_eq!(
        arena.alloc_with(0x10, 1, Policy::NextFit).expect("wrap"),
        0x00,
        "next fit wraps to the lowest address when the cursor is past the last span"
    );
}

#[test]
fn coalescing_merges_three_neighbours() {
    let mut arena = Vmem::new(0x1000, 0x300).expect("arena");
    let a = arena.alloc(0x100, 1).expect("a");
    let b = arena.alloc(0x100, 1).expect("b");
    let c = arena.alloc(0x100, 1).expect("c");
    assert_eq!((a, b, c), (0x1000, 0x1100, 0x1200));
    arena.free(b, 0x100).expect("free b");
    assert_eq!(arena.free_span_count(), 1);
    arena.free(a, 0x100).expect("free a");
    assert_eq!(arena.free_span_count(), 1, "a and b merge");
    assert_eq!(arena.total_free(), 0x200);
    arena.free(c, 0x100).expect("free c");
    assert_eq!(arena.span_count(), 1);
    assert_eq!(arena.largest_free(), 0x300);
}

#[test]
fn tag_exhaustion_is_transactional() {
    let mut arena = Vmem::with_capacity(0x1000, 0x100, 2).expect("arena");
    assert_eq!(arena.tag_capacity(), 2);
    let first = arena.alloc(0x10, 1).expect("first split");
    assert_eq!(arena.span_count(), 2);
    let stats = arena.stats();
    assert_eq!(arena.alloc(0x10, 1), Err(VmemError::NoTags));
    assert_eq!(arena.span_count(), stats.spans);
    assert_eq!(arena.total_free(), stats.free_bytes);
    assert_eq!(arena.stats().alloc_fail, 1);
    arena.free(first, 0x10).expect("free first");
    assert_eq!(arena.span_count(), 1);
    assert_eq!(arena.total_free(), 0x100);
}

#[test]
fn import_coalesces_without_a_new_tag() {
    let mut arena = Vmem::with_capacity(0x1000, 0x100, 1).expect("arena");
    arena
        .import(0x1100, 0x100)
        .expect("adjacent import reuses the free tag");
    assert_eq!(arena.span_count(), 1);
    assert_eq!(arena.total_free(), 0x200);
    assert!(arena.contains(0x11ff));
    assert_eq!(
        arena.import(0x2000, 0x100),
        Err(VmemError::NoTags),
        "a disjoint import needs a second tag"
    );
    assert!(!arena.contains(0x2000));
}

#[test]
fn fragmentation_reports_scatter() {
    let mut arena = Vmem::new(0, 0x300).expect("arena");
    assert_eq!(arena.alloc(0x100, 1).expect("low"), 0);
    assert_eq!(arena.alloc(0x100, 1).expect("middle"), 0x100);
    arena.free(0, 0x100).expect("free low");
    assert_eq!(arena.fragmentation(), 0.5);
    arena.free(0x100, 0x100).expect("free middle");
    assert_eq!(arena.fragmentation(), 0.0);
    assert_eq!(arena.alloc(0x300, 1).expect("all"), 0);
    assert_eq!(arena.fragmentation(), 1.0);
}

#[test]
fn stats_and_segment_views() {
    let mut arena = Vmem::new(0x1000, 0x100).expect("arena");
    let base = arena.alloc(0x20, 1).expect("alloc");
    arena.free(base, 0x20).expect("free");
    let stats = arena.stats();
    assert_eq!(stats.allocs, 1);
    assert_eq!(stats.frees, 1);
    assert_eq!(stats.alloc_fail, 0);
    assert_eq!(stats.spans, 1);
    assert_eq!(stats.free_bytes, 0x100);
    assert_eq!(stats.used_bytes, 0);
    assert_eq!(stats.largest_free, 0x100);
    assert_eq!(arena.free_segments().count(), 1);
    assert_eq!(arena.allocated_segments().count(), 0);
    assert_eq!(arena.tag_capacity(), DEFAULT_TAG_CAPACITY);
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Alloc(u64, u64),
    AllocAt(u64, u64, u64),
    Free(usize),
}

const ACTIONS: [Op; 10] = [
    Op::Alloc(16, 1),
    Op::Alloc(32, 1),
    Op::Alloc(16, 16),
    Op::Alloc(64, 8),
    Op::AllocAt(0x1000, 16, 1),
    Op::AllocAt(0x1040, 16, 16),
    Op::Free(0),
    Op::Free(1),
    Op::Free(2),
    Op::Free(3),
];

fn apply(
    arena: &mut Vmem,
    slots: &mut [Option<(u64, u64)>; 4],
    live: &mut Vec<(u64, u64)>,
    op: Op,
) {
    match op {
        Op::Alloc(size, align) => {
            if let Ok(base) = arena.alloc(size, align) {
                let slot = slots.iter().position(Option::is_none).expect("a free slot");
                slots[slot] = Some((base, size));
                live.push((base, size));
            }
        }
        Op::AllocAt(addr, size, align) => {
            if arena.alloc_at(addr, size, align).is_ok() {
                let slot = slots.iter().position(Option::is_none).expect("a free slot");
                slots[slot] = Some((addr, size));
                live.push((addr, size));
            }
        }
        Op::Free(slot) => {
            if let Some((base, size)) = slots[slot].take() {
                arena.free(base, size).expect("free live block");
                let position = live
                    .iter()
                    .position(|&range| range == (base, size))
                    .expect("live block");
                live.swap_remove(position);
            }
        }
    }
}

#[test]
fn exhaustive_alloc_free_coalesce() {
    const START: u64 = 0x1000;
    const END: u64 = 0x1100;
    const DEPTH: u32 = 4;
    let total = (ACTIONS.len() as u32).pow(DEPTH);
    for code in 0..total {
        let mut arena = Vmem::with_capacity(START, END - START, 32).expect("arena");
        let mut slots: [Option<(u64, u64)>; 4] = [None; 4];
        let mut live: Vec<(u64, u64)> = Vec::new();
        let mut value = code;
        for _ in 0..DEPTH {
            let action = ACTIONS[(value % ACTIONS.len() as u32) as usize];
            value /= ACTIONS.len() as u32;
            apply(&mut arena, &mut slots, &mut live, action);
            check_all(&arena, START, END, &live);
        }
    }
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
fn property_no_overlapping_live_segments() {
    const START: u64 = 0x10_0000;
    const SIZE: u64 = 0x4000;
    const ROUNDS: usize = 4000;
    let mut arena = Vmem::with_capacity(START, SIZE, 256).expect("arena");
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut live: Vec<(u64, u64)> = Vec::new();

    for _ in 0..ROUNDS {
        match rng.below(100) {
            0..=54 => {
                let size = 1 + rng.below(1024);
                let align = 1u64 << rng.below(8);
                if let Ok(base) = arena.alloc(size, align) {
                    assert_eq!(base & (align - 1), 0);
                    assert!(arena.contains_range(base, size));
                    live.push((base, size));
                }
            }
            55..=69 => {
                let align = 1u64 << rng.below(13);
                let addr = START + rng.below(SIZE / align) * align;
                let size = 1 + rng.below(256);
                if arena.alloc_at(addr, size, align).is_ok() {
                    live.push((addr, size));
                }
            }
            _ => {
                if !live.is_empty() {
                    let idx = rng.below(live.len() as u64) as usize;
                    let (base, size) = live.swap_remove(idx);
                    arena.free(base, size).expect("live block");
                }
            }
        }
        check_all(&arena, START, START + SIZE, &live);
        assert!(live.len() <= arena.tag_capacity());
    }

    while !live.is_empty() {
        let idx = rng.below(live.len() as u64) as usize;
        let (base, size) = live.swap_remove(idx);
        arena.free(base, size).expect("live block");
        check_all(&arena, START, START + SIZE, &live);
    }

    assert_eq!(arena.span_count(), 1);
    assert_eq!(arena.largest_free(), SIZE);
    assert_eq!(arena.total_free(), SIZE);
    assert_eq!(arena.total_allocated(), 0);
    assert!(arena.fragmentation() < f32::EPSILON);
    assert_eq!(arena.free_span_count(), 1);
    assert_eq!(arena.allocated_span_count(), 0);
}
