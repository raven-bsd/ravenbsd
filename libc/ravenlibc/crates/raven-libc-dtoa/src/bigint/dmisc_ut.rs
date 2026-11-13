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

/// Tiny `Vec<u32>` model of a big integer.
fn model_value(words: &[u32]) -> u128 {
    let mut value = 0u128;
    for (i, word) in words.iter().enumerate() {
        value |= u128::from(*word) << (32 * i);
    }
    value
}

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

#[test]
fn quorem_matches_long_division() {
    // The caller of quorem guarantees that the quotient digit is at most 9.
    let cases: &[(&[u32], &[u32])] = &[
        (&[10], &[3]),
        (&[100], &[11]),
        (&[0xffff_ffff], &[0x2000_0000]),
        (&[0xffff_ffff, 1], &[0x2000_0000, 1]),
        (&[3], &[3]),
        (&[0], &[7]),
    ];
    for (b_words, s_words) in cases {
        let mut ctx = DtoaCtx::new();
        let b = make(&mut ctx, b_words);
        let s = make(&mut ctx, s_words);
        let want_q = model_value(b_words) / model_value(s_words);
        let want_r = model_value(b_words) % model_value(s_words);
        let q = quorem(&mut ctx, b, s);
        assert_eq!(
            u128::from(q as u32),
            want_q,
            "quotient digit for {b_words:?}"
        );
        let mut got = 0u128;
        for i in 0..ctx.wds(b) {
            got |= u128::from(ctx.x(b, i as u32)) << (32 * i as u128);
        }
        assert_eq!(got, want_r, "remainder for {b_words:?}");
    }
}
