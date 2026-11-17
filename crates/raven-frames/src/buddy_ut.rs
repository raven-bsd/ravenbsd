// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors

//! Tests for the buddy/zone frame allocator.

use super::*;

/// 128-frame buddy: orders 0..=7.
const ORDERS: usize = 8;
const MAX_BLOCKS: usize = 128;

/// Declare the descriptor storage and the allocator over it in the caller's scope.
macro_rules! buddy {
    ($storage:ident, $allocator:ident, $frames:expr) => {
        let mut $storage = [Block::empty(); MAX_BLOCKS];
        let mut $allocator =
            BuddyAllocator::<ORDERS, NoPager>::with_pager(&mut $storage, 0, $frames, NoPager)
                .expect("valid buddy");
    };
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

fn assert_conserved<'a, const O: usize>(
    allocator: &BuddyAllocator<'a, O, NoPager>,
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
    buddy!(storage, allocator, 64);
    let a = allocator.alloc(0).expect("alloc");
    let b = allocator.alloc(0).expect("alloc");
    assert_ne!(a, b);
    allocator.free(a, 0).expect("free");
    allocator.free(b, 0).expect("free");
    assert_eq!(allocator.stats().free, 64);
}

#[test]
fn coalescing_merges_buddies() {
    buddy!(storage, allocator, 64);
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
    buddy!(storage, allocator, 8); // top order 3
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
    buddy!(storage, allocator, 16);
    let a = allocator.alloc(1).expect("alloc");
    allocator.free(a, 1).expect("free");
    assert_eq!(allocator.free(a, 1), Err(BuddyError::DoubleFree));
    assert_eq!(allocator.free(100, 0), Err(BuddyError::OutOfRange));
    assert_eq!(allocator.free(1, 1), Err(BuddyError::Misaligned));
    assert_eq!(allocator.alloc(8), Err(BuddyError::OrderTooLarge));
}

#[test]
fn alloc_at_places_exactly() {
    buddy!(storage, allocator, 32);
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
    buddy!(storage, allocator, 16);
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
    buddy!(storage, allocator, 16);
    let a = allocator.alloc(1).expect("alloc");
    assert_eq!(allocator.reserve(0, 4), Err(BuddyError::Occupied));
    allocator.free(a, 1).expect("free");
    allocator.reserve(0, 4).expect("now free");
}

#[test]
fn pager_failure_injection_rolls_back() {
    let mut storage = [Block::empty(); MAX_BLOCKS];
    let mut allocator =
        BuddyAllocator::<ORDERS, FailingPager>::with_pager(&mut storage, 0, 32, FailingPager)
            .expect("buddy");
    assert_eq!(allocator.alloc(0), Err(BuddyError::Pager));
    assert_eq!(allocator.alloc(2), Err(BuddyError::Pager));
    // Nothing leaked: the whole range is still free.
    assert_eq!(allocator.stats().free, 32);
    assert_eq!(allocator.stats().alloc_fail, 2);
}

#[test]
fn stats_accounting() {
    buddy!(storage, allocator, 64);
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
        let mut storage = [Block::empty(); 4];
        let mut allocator =
            BuddyAllocator::<3, NoPager>::with_pager(&mut storage, 0, 4, NoPager).expect("buddy");
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
    buddy!(storage, allocator, 128);
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
    buddy!(storage, allocator, 16);
    for _ in 0..4 {
        allocator.alloc(2).expect("four order-2 blocks fit");
    }
    assert_eq!(allocator.alloc(2), Err(BuddyError::OutOfFrames));
    assert_eq!(allocator.alloc(3), Err(BuddyError::OutOfFrames));
}

#[test]
fn state_reports_free_allocated_and_reserved() {
    buddy!(storage, allocator, 16);
    assert_eq!(allocator.state(0), Some(FrameState::Free));
    let frame = allocator.alloc(0).expect("alloc");
    assert_eq!(allocator.state(frame), Some(FrameState::Allocated));
    allocator.reserve(8, 2).expect("reserve");
    assert_eq!(allocator.state(8), Some(FrameState::Reserved));
    assert_eq!(allocator.state(16), None);
    allocator.free(frame, 0).expect("free");
    assert_eq!(allocator.state(frame), Some(FrameState::Free));
}

#[test]
fn release_frees_reserved_and_allocated_frames() {
    buddy!(storage, allocator, 16);
    allocator.reserve(4, 4).expect("reserve");
    assert_eq!(allocator.stats().reserved, 4);
    for frame in 4..8 {
        allocator.release(frame).expect("release reserved");
    }
    assert_eq!(allocator.stats().reserved, 0);
    // The released run is allocatable again and coalesces with the rest.
    assert_eq!(allocator.alloc(3), Ok(0));
    // Releasing an allocated order-0 frame works like free.
    allocator.free(0, 3).expect("free order 3");
    let single = allocator.alloc(0).expect("alloc");
    allocator.release(single).expect("release allocated");
    assert_eq!(allocator.state(single), Some(FrameState::Free));
    // Releasing a free frame reports a double free.
    assert_eq!(allocator.release(single), Err(BuddyError::DoubleFree));
}

#[test]
fn alloc_contiguous_places_an_aligned_run() {
    buddy!(storage, allocator, 64);
    // Make frame 0 reserved so the first aligned run is not trivially zero.
    allocator.reserve(0, 1).expect("reserve");
    let start = allocator.alloc_contiguous(4, 4, 64).expect("run");
    assert_eq!(start, 4);
    assert_eq!(start % 4, 0);
    for frame in start..start + 4 {
        assert_eq!(allocator.state(frame), Some(FrameState::Allocated));
    }
    // Each frame is independently freeable, exactly as the DMA teardown expects.
    for frame in start..start + 4 {
        allocator.free(frame, 0).expect("free frame");
    }
    assert_eq!(allocator.stats().free, 63);
}

#[test]
fn alloc_contiguous_skips_occupied_runs() {
    buddy!(storage, allocator, 32);
    allocator.reserve(0, 8).expect("reserve");
    allocator.alloc_at(8, 0).expect("occupy 8");
    let start = allocator.alloc_contiguous(4, 4, 32).expect("run");
    assert!(start >= 12, "run must avoid the occupied frame: {start}");
    for frame in start..start + 4 {
        allocator.free(frame, 0).expect("free frame");
    }
}

#[test]
fn alloc_contiguous_rejects_bad_arguments() {
    buddy!(storage, allocator, 16);
    assert_eq!(
        allocator.alloc_contiguous(0, 1, 16),
        Err(BuddyError::ZeroSize)
    );
    assert_eq!(
        allocator.alloc_contiguous(1, 3, 16),
        Err(BuddyError::Misaligned)
    );
    assert_eq!(
        allocator.alloc_contiguous(17, 1, 16),
        Err(BuddyError::OutOfFrames)
    );
    // A run that cannot fit below max_frame fails cleanly.
    assert_eq!(
        allocator.alloc_contiguous(8, 8, 4),
        Err(BuddyError::OutOfFrames)
    );
}

/// A 1,000,000-frame model: the kernel's largest supported range, exercised through the buddy with
/// alloc/free and conservation checks. The inline block array is 8 MiB, so the test runs on a
/// thread with a 64 MiB stack instead of the default test stack.
#[test]
fn million_frame_model() {
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(|| {
            const BIG_ORDERS: usize = 21;
            const BIG_BLOCKS: usize = 1 << 20;
            let mut storage = [Block::zeroed(); BIG_BLOCKS];
            let mut allocator = BuddyAllocator::<BIG_ORDERS, NoPager>::with_pager(
                &mut storage,
                0,
                1 << 20,
                NoPager,
            )
            .expect("1M-frame buddy");
            let mut rng = Rng(0x00c0_ffee_1234_5678);
            let mut live: Vec<u64> = Vec::new();
            for _ in 0..200_000 {
                if live.is_empty() || rng.below(100) < 55 {
                    if let Ok(frame) = allocator.alloc(0) {
                        live.push(frame);
                    }
                } else {
                    let index = rng.below(live.len() as u64) as usize;
                    let frame = live.swap_remove(index);
                    allocator.free(frame, 0).expect("free live");
                }
                if live.len().is_multiple_of(1000) {
                    let stats = allocator.stats();
                    assert_eq!(stats.free + live.len() as u64, stats.total, "conservation");
                }
            }
            for frame in live.drain(..) {
                allocator.free(frame, 0).expect("free remaining");
            }
            let stats = allocator.stats();
            assert_eq!(stats.free, 1 << 20);
            assert_eq!(allocator.free_blocks(20), 1, "full coalescing");
        })
        .expect("spawn")
        .join()
        .expect("model thread");
}
