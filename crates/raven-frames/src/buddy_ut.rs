// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the buddy/zone frame allocator.

use super::*;

/// 128-frame buddy: orders 0..=7.
const ORDERS: usize = 8;
const MAX_BLOCKS: usize = 128;

type Buddy = BuddyAllocator<ORDERS, MAX_BLOCKS, NoPager>;

fn buddy(frames: u64) -> Buddy {
    Buddy::new(0, frames).expect("valid buddy")
}

/// A pager whose `back` always fails, for failure injection.
struct FailingPager;

impl Pager for FailingPager {
    fn back(&mut self, _frame: u64, _count: u64) -> bool {
        false
    }

    fn unback(&mut self, _frame: u64, _count: u64) {}
}

fn allocated_frames(live: &[(u64, usize)]) -> u64 {
    live.iter().map(|&(_, order)| 1u64 << order).sum()
}

fn assert_conserved<const O: usize, const B: usize>(
    allocator: &BuddyAllocator<O, B, NoPager>,
    live: &[(u64, usize)],
) {
    let stats = allocator.stats();
    assert_eq!(
        stats.free + allocated_frames(live) + stats.reserved,
        stats.total,
        "frame conservation violated"
    );
    assert_eq!(stats.allocs - stats.frees, live.len() as u64);
    // Live blocks are disjoint and in range.
    let mut sorted: Vec<(u64, u64)> = live
        .iter()
        .map(|&(frame, order)| (frame, 1u64 << order))
        .collect();
    sorted.sort_unstable();
    for pair in sorted.windows(2) {
        assert!(pair[0].0 + pair[0].1 <= pair[1].0, "live blocks overlap");
    }
}

#[test]
fn alloc_and_free_round_trip() {
    let mut allocator = buddy(64);
    let a = allocator.alloc(0).expect("alloc");
    let b = allocator.alloc(0).expect("alloc");
    assert_ne!(a, b);
    allocator.free(a, 0).expect("free");
    allocator.free(b, 0).expect("free");
    assert_eq!(allocator.stats().free, 64);
}

#[test]
fn coalescing_merges_buddies() {
    let mut allocator = buddy(64);
    let a = allocator.alloc(0).expect("a");
    let b = allocator.alloc(0).expect("b");
    let c = allocator.alloc(0).expect("c");
    let d = allocator.alloc(0).expect("d");
    assert_eq!((a, b, c, d), (0, 1, 2, 3));
    // Freeing a then b coalesces them into [0,2) at order 1; [2,4) is still allocated.
    allocator.free(a, 0).expect("free a");
    allocator.free(b, 0).expect("free b");
    assert_eq!(allocator.free_blocks(1), 1);
    assert_eq!(allocator.free_blocks(0), 0);
    // Freeing the rest coalesces everything back to one top-order block.
    allocator.free(c, 0).expect("free c");
    allocator.free(d, 0).expect("free d");
    assert_eq!(allocator.free_blocks(6), 1);
    assert_eq!(allocator.stats().free, 64);
}

#[test]
fn alloc_splits_larger_blocks() {
    let mut allocator = buddy(8); // top order 3
    // First order-0 alloc splits the order-3 block all the way down.
    let a = allocator.alloc(0).expect("alloc");
    assert_eq!(a, 0);
    // An order-3 alloc of the whole range should now fail (one frame is taken).
    assert_eq!(allocator.alloc(3), Err(BuddyError::OutOfFrames));
    allocator.free(a, 0).expect("free");
    // Now the whole range is coalesced back into one order-3 block.
    assert_eq!(allocator.free_blocks(3), 1);
    assert_eq!(allocator.alloc(3), Ok(0));
}

#[test]
fn double_free_and_out_of_range_are_rejected() {
    let mut allocator = buddy(16);
    let a = allocator.alloc(1).expect("alloc");
    allocator.free(a, 1).expect("free");
    assert_eq!(allocator.free(a, 1), Err(BuddyError::DoubleFree));
    assert_eq!(allocator.free(100, 0), Err(BuddyError::OutOfRange));
    assert_eq!(allocator.free(1, 1), Err(BuddyError::Misaligned));
    assert_eq!(allocator.alloc(8), Err(BuddyError::OrderTooLarge));
}

#[test]
fn alloc_at_places_exactly() {
    let mut allocator = buddy(32);
    allocator.alloc_at(16, 1).expect("place at 16");
    assert!(allocator.alloc(2).is_ok(), "the rest still allocates");
    assert_eq!(
        allocator.alloc_at(16, 1),
        Err(BuddyError::Occupied),
        "already allocated"
    );
    assert_eq!(
        allocator.alloc_at(3, 1),
        Err(BuddyError::Misaligned),
        "frame 3 is not order-1 aligned"
    );
    assert_eq!(allocator.alloc_at(40, 1), Err(BuddyError::OutOfRange));
    allocator.free(16, 1).expect("free placement");
}

#[test]
fn reserve_marks_frames_unallocatable() {
    let mut allocator = buddy(16);
    allocator.reserve(0, 4).expect("reserve low frames");
    assert_eq!(allocator.stats().reserved, 4);
    // The reserved frames are never handed out; the rest is still available.
    for _ in 0..12 {
        let frame = allocator.alloc(0).expect("alloc");
        assert!(frame >= 4, "reserved frame handed out: {frame}");
    }
    assert_eq!(allocator.alloc(0), Err(BuddyError::OutOfFrames));
}

#[test]
fn reserve_over_allocated_is_rejected() {
    let mut allocator = buddy(16);
    let a = allocator.alloc(1).expect("alloc");
    assert_eq!(allocator.reserve(0, 4), Err(BuddyError::Occupied));
    allocator.free(a, 1).expect("free");
    allocator.reserve(0, 4).expect("now free");
}

#[test]
fn pager_failure_injection_rolls_back() {
    let mut allocator =
        BuddyAllocator::<ORDERS, MAX_BLOCKS, FailingPager>::with_pager(0, 32, FailingPager)
            .expect("buddy");
    assert_eq!(allocator.alloc(0), Err(BuddyError::Pager));
    assert_eq!(allocator.alloc(2), Err(BuddyError::Pager));
    // Nothing leaked: the whole range is still free.
    assert_eq!(allocator.stats().free, 32);
    assert_eq!(allocator.stats().alloc_fail, 2);
}

#[test]
fn stats_accounting() {
    let mut allocator = buddy(64);
    let a = allocator.alloc(2).expect("alloc order 2");
    let b = allocator.alloc(0).expect("alloc order 0");
    let stats = allocator.stats();
    assert_eq!(stats.total, 64);
    assert_eq!(stats.free, 64 - 4 - 1);
    assert_eq!(stats.allocs, 2);
    allocator.free(a, 2).expect("free");
    allocator.free(b, 0).expect("free");
    assert_eq!(allocator.stats().free, 64);
    assert_eq!(allocator.stats().frees, 2);
}

#[test]
fn exhaustive_small_model() {
    // A 4-frame buddy; enumerate all depth-3 sequences of the four actions and check the
    // conservation/coalescing invariants after every step.
    const OPS: usize = 4;
    for code in 0..(OPS as u32).pow(3) {
        let mut allocator = BuddyAllocator::<3, 4, NoPager>::new(0, 4).expect("buddy");
        let mut live: Vec<(u64, usize)> = Vec::new();
        let mut value = code;
        for _ in 0..3 {
            match value % OPS as u32 {
                0 => {
                    if let Ok(frame) = allocator.alloc(0) {
                        live.push((frame, 0));
                    }
                }
                1 => {
                    if let Ok(frame) = allocator.alloc(1) {
                        live.push((frame, 1));
                    }
                }
                2 => {
                    if let Some(index) = live.iter().position(|&(_, order)| order == 0) {
                        let (frame, order) = live.swap_remove(index);
                        allocator.free(frame, order).expect("free live");
                    }
                }
                _ => {
                    if let Some(index) = live.iter().position(|&(_, order)| order == 1) {
                        let (frame, order) = live.swap_remove(index);
                        allocator.free(frame, order).expect("free live");
                    }
                }
            }
            value /= OPS as u32;
            assert_conserved(&allocator, &live);
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
fn random_alloc_free_model() {
    let mut allocator = buddy(128);
    let mut rng = Rng(0x0123_4567_89ab_cdef);
    let mut live: Vec<(u64, usize)> = Vec::new();

    for _ in 0..20_000 {
        if live.is_empty() || rng.below(100) < 60 {
            let order = rng.below(8) as usize;
            if let Ok(frame) = allocator.alloc(order) {
                assert_eq!(frame % (1 << order), 0);
                live.push((frame, order));
            }
        } else {
            let index = rng.below(live.len() as u64) as usize;
            let (frame, order) = live.swap_remove(index);
            allocator.free(frame, order).expect("free live");
        }
        if live.len() < 8 {
            assert_conserved(&allocator, &live);
        }
    }

    for (frame, order) in live.drain(..) {
        allocator.free(frame, order).expect("free remaining");
    }
    assert_eq!(allocator.stats().free, 128);
    assert_eq!(
        allocator.free_blocks(7),
        1,
        "everything coalesces back to one block"
    );
    assert_eq!(allocator.stats().frees, allocator.stats().allocs);
}

#[test]
fn oom_returns_a_real_error() {
    let mut allocator = buddy(16);
    for _ in 0..4 {
        allocator.alloc(2).expect("four order-2 blocks fit");
    }
    assert_eq!(allocator.alloc(2), Err(BuddyError::OutOfFrames));
    assert_eq!(allocator.alloc(3), Err(BuddyError::OutOfFrames));
}
