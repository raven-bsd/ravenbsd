// SPDX-License-Identifier: BSD-3-Clause
// Copyright (c) 2026, Bo Maryniuk and RavenBSD contributors
#![allow(
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used
)]

use super::*;
use crate::config::KMAX;

fn write_pattern(ctx: &mut DtoaCtx, r: BigRef, wds: i32) {
    ctx.set_wds(r, wds);
    for i in 0..wds {
        ctx.set_x(r, i as u32, 0x1111_0000 ^ i as u32);
    }
}

#[test]
fn pool_words_match_published_size() {
    assert_eq!(POOL_WORDS * 4, crate::POOL_BYTES);
    assert_eq!(DtoaCtx::new().peak_words(), 0);
    assert!(!DtoaCtx::new().failed());
}

#[test]
fn arena_alloc_free_reuse() {
    for k in 0..=KMAX {
        let mut ctx = DtoaCtx::new();
        let first = ctx.balloc(k).expect("first allocation");
        assert_eq!(ctx.k(first), k);
        assert_eq!(ctx.maxwds(first), 1 << k);
        let wds = ctx.maxwds(first);
        write_pattern(&mut ctx, first, wds);
        let peak = ctx.peak_words();
        ctx.bfree(first);
        let second = ctx.balloc(k).expect("reuse");
        assert_eq!(first, second, "size class {k} reuses the freed block");
        assert_eq!(ctx.peak_words(), peak, "reuse must not grow the arena");
        assert_eq!(ctx.k(second), k);
        for i in 0..wds {
            assert_eq!(ctx.x(second, i as u32), 0x1111_0000 ^ i as u32);
        }
        ctx.bfree(second);
    }
}

#[test]
fn arena_all_classes_and_large_blocks() {
    let mut ctx = DtoaCtx::new();
    let mut blocks = Vec::new();
    for k in 0..=KMAX {
        let r = ctx.balloc(k).expect("small class fits in 8 KiB");
        blocks.push(r);
    }
    for r in blocks.iter().rev() {
        ctx.bfree(*r);
    }
    // A freed size class is reused from the free list without growing the arena.
    let before = ctx.peak_words();
    let again = ctx.balloc(KMAX).expect("reused from the free list");
    assert_eq!(ctx.k(again), KMAX);
    assert_eq!(ctx.peak_words(), before);
    ctx.bfree(again);
    // k = 10 is 1024 words plus the header: it fits in a fresh pool by bump allocation, and the
    // last-allocation rollback lets the next block reuse the space.
    let mut fresh = DtoaCtx::new();
    let big = fresh.balloc(10).expect("1024-word block fits the pool");
    assert_eq!(fresh.k(big), 10);
    let before = fresh.peak_words();
    fresh.bfree(big);
    assert_eq!(fresh.peak_words(), before);
    let small = fresh.balloc(8).expect("after rollback");
    assert_eq!(fresh.k(small), 8);
}

#[cfg(feature = "alloc")]
#[test]
fn arena_spill_for_pathological_sizes() {
    let mut ctx = DtoaCtx::new();
    let r = ctx.balloc(16).expect("spill holds 65536 words");
    assert_eq!(ctx.k(r), 16);
    assert!(ctx.peak_words() <= POOL_WORDS);
    ctx.set_x(r, 0, 0xdead_beef);
    assert_eq!(ctx.x(r, 0), 0xdead_beef);
    ctx.bfree(r);
}

#[cfg(not(feature = "alloc"))]
#[test]
fn arena_nomemory_for_pathological_sizes() {
    let mut ctx = DtoaCtx::new();
    assert!(ctx.balloc(16).is_none());
    assert!(ctx.failed());
}

#[test]
fn arena_every_block_inside_the_pool() {
    let mut ctx = DtoaCtx::new();
    for k in 0..=14 {
        match ctx.balloc(k) {
            Some(r) => {
                if (r.0 as usize) < POOL_WORDS {
                    let end = (r.0 as usize) + 5 + (1usize << k);
                    assert!(end <= POOL_WORDS, "block k={k} end {end}");
                } else {
                    #[cfg(not(feature = "alloc"))]
                    panic!("spill index without alloc");
                }
                #[cfg(not(feature = "alloc"))]
                ctx.bfree(r);
            }
            None => {
                assert!(ctx.failed());
            }
        }
    }
}

#[test]
fn ctx_is_send() {
    fn assert_send<T: Send>() {}
    assert_send::<DtoaCtx>();
}

#[test]
fn ctx_default_is_new() {
    let mut a = DtoaCtx::default();
    let mut b = DtoaCtx::new();
    let ra = a.balloc(3).unwrap();
    let rb = b.balloc(3).unwrap();
    assert_eq!(ra, rb);
}

#[test]
fn kmax_is_the_published_nine() {
    // RD-31 fixes Kmax at 9 (the 8 KiB pool makes larger requests spill or fail). A deliberate
    // change to this constant must be caught here even if the allocator happens to cope.
    assert_eq!(KMAX, 9);
}
