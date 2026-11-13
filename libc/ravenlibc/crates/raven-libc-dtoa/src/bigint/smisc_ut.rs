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
fn match_prefix_is_case_insensitive_and_commits_on_success() {
    let mut pos = 0usize;
    assert!(match_prefix(b"Infinity", &mut pos, b"nf"));
    assert_eq!(pos, 3);
    pos = 2;
    assert!(match_prefix(b"Infinity", &mut pos, b"inity"));
    assert_eq!(pos, 8);
    pos = 0;
    assert!(!match_prefix(b"Inx", &mut pos, b"nf"));
    assert_eq!(pos, 0, "failure leaves the cursor");
    let mut pos = 0;
    assert!(match_prefix(b"NaN(1)", &mut pos, b"an"));
}

#[test]
fn copybits_and_any_on() {
    let mut ctx = DtoaCtx::new();
    let b = ctx.balloc(1).unwrap();
    ctx.set_wds(b, 2);
    ctx.set_x(b, 0, 0xffff_ffff);
    ctx.set_x(b, 1, 0x0000_0001);
    let mut out = [0u32; 3];
    copybits(&mut out, 53, b, &ctx);
    assert_eq!(out, [0xffff_ffff, 0x0000_0001, 0]);
    assert_eq!(any_on(&ctx, b, 32), 1);
    assert_eq!(any_on(&ctx, b, 0), 0);
    ctx.set_x(b, 1, 0x8000_0001);
    assert_eq!(any_on(&ctx, b, 63), 1);
    assert_eq!(any_on(&ctx, b, 32), 1);
}

#[test]
fn s2b_and_ratio() {
    let mut ctx = DtoaCtx::new();
    let b = s2b(&mut ctx, b"12345", 5, 5, 12345, 0).unwrap();
    assert_eq!(ctx.wds(b), 1);
    assert_eq!(ctx.x(b, 0), 12345);
    let mut ctx = DtoaCtx::new();
    let one = s2b(&mut ctx, b"1", 1, 1, 1, 0).unwrap();
    let two = s2b(&mut ctx, b"2", 1, 1, 2, 0).unwrap();
    assert_eq!(ratio(&ctx, one, two), 0.5);
}
