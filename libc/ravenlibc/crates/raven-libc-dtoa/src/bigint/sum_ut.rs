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
use crate::ctx::DtoaCtx;

fn make(ctx: &mut DtoaCtx, words: &[u32]) -> BigRef {
    let mut k = 0i32;
    while (1usize << k) < words.len().max(1) {
        k += 1;
    }
    let b = ctx.balloc(k).unwrap();
    for (i, word) in words.iter().enumerate() {
        ctx.set_x(b, i as u32, *word);
    }
    ctx.set_wds(b, words.len() as i32);
    b
}

fn words(ctx: &DtoaCtx, b: BigRef) -> Vec<u32> {
    (0..ctx.wds(b)).map(|i| ctx.x(b, i as u32)).collect()
}

#[test]
fn sum_adds_and_carries() {
    let mut ctx = DtoaCtx::new();
    let a = make(&mut ctx, &[0xffff_ffff, 0xffff_ffff]);
    let b = make(&mut ctx, &[1]);
    let c = sum(&mut ctx, a, b).unwrap();
    assert_eq!(words(&ctx, c), vec![0, 0, 1]);
    let mut ctx = DtoaCtx::new();
    let a = make(&mut ctx, &[0x1234_5678]);
    let b = make(&mut ctx, &[0x8765_4321]);
    let c = sum(&mut ctx, a, b).unwrap();
    assert_eq!(words(&ctx, c), vec![0x9999_9999]);
}
