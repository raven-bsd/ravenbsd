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

#[test]
fn rshift_shifts_in_place() {
    let mut ctx = DtoaCtx::new();
    let b = ctx.balloc(1).unwrap();
    ctx.set_wds(b, 2);
    ctx.set_x(b, 0, 0x8000_0000);
    ctx.set_x(b, 1, 0x0000_0001);
    rshift(&mut ctx, b, 32);
    assert_eq!(ctx.wds(b), 1);
    assert_eq!(ctx.x(b, 0), 0x0000_0001);
    rshift(&mut ctx, b, 1);
    assert_eq!(ctx.wds(b), 0);
    assert_eq!(ctx.x(b, 0), 0);
    assert_eq!(trailz(&ctx, b), 0);
}

#[test]
fn trailz_counts_zero_words() {
    let mut ctx = DtoaCtx::new();
    let b = ctx.balloc(2).unwrap();
    ctx.set_wds(b, 3);
    ctx.set_x(b, 0, 0);
    ctx.set_x(b, 1, 0);
    ctx.set_x(b, 2, 0x0000_0010);
    assert_eq!(trailz(&ctx, b), 68);
    ctx.set_x(b, 0, 0x0000_0100);
    assert_eq!(trailz(&ctx, b), 8);
}
