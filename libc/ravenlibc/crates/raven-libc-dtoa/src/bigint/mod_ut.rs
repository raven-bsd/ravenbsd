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
fn bcopy_copies_sign_wds_and_words() {
    let mut ctx = DtoaCtx::new();
    let src = ctx.balloc(2).unwrap();
    let dst = ctx.balloc(2).unwrap();
    ctx.set_sign(src, 1);
    ctx.set_wds(src, 3);
    ctx.set_x(src, 0, 0x1111_1111);
    ctx.set_x(src, 1, 0x2222_2222);
    ctx.set_x(src, 2, 0x3333_3333);
    bcopy(&mut ctx, dst, src);
    assert_eq!(ctx.sign(dst), 1);
    assert_eq!(ctx.wds(dst), 3);
    assert_eq!(ctx.x(dst, 0), 0x1111_1111);
    assert_eq!(ctx.x(dst, 1), 0x2222_2222);
    assert_eq!(ctx.x(dst, 2), 0x3333_3333);
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn model_trim(mut v: Vec<u32>) -> Vec<u32> {
    while v.len() > 1 && v.last() == Some(&0) {
        v.pop();
    }
    v
}

fn model_from(ctx: &mut DtoaCtx, v: &[u32]) -> BigRef {
    let r = ctx.balloc(4).expect("model block");
    ctx.set_wds(r, v.len() as i32);
    for (i, &word) in v.iter().enumerate() {
        ctx.set_x(r, i as u32, word);
    }
    ctx.set_sign(r, 0);
    r
}

fn model_of(ctx: &DtoaCtx, r: BigRef) -> Vec<u32> {
    let wds = ctx.wds(r) as usize;
    if wds == 0 {
        return vec![0];
    }
    let mut out = Vec::with_capacity(wds);
    for i in 0..wds {
        out.push(ctx.x(r, i as u32));
    }
    model_trim(out)
}

fn model_mul_small(v: &[u32], m: u32, a: u32) -> Vec<u32> {
    let mut out = Vec::with_capacity(v.len() + 1);
    let mut carry = u64::from(a);
    for &x in v {
        let p = u64::from(x) * u64::from(m) + carry;
        out.push(p as u32);
        carry = p >> 32;
    }
    if carry != 0 {
        out.push(carry as u32);
    }
    model_trim(out)
}

fn model_mul(a: &[u32], b: &[u32]) -> Vec<u32> {
    let mut out = vec![0u32; a.len() + b.len()];
    for (i, &x) in a.iter().enumerate() {
        let mut carry = 0u64;
        for (j, &y) in b.iter().enumerate() {
            let p = u64::from(x) * u64::from(y) + u64::from(out[i + j]) + carry;
            out[i + j] = p as u32;
            carry = p >> 32;
        }
        out[i + b.len()] = carry as u32;
    }
    model_trim(out)
}

fn model_pow5(v: &[u32], k: u32) -> Vec<u32> {
    let mut out = v.to_vec();
    for _ in 0..k {
        out = model_mul_small(&out, 5, 0);
    }
    out
}

fn model_cmp(a: &[u32], b: &[u32]) -> i32 {
    let a = model_trim(a.to_vec());
    let b = model_trim(b.to_vec());
    if a.len() != b.len() {
        return if a.len() < b.len() { -1 } else { 1 };
    }
    for i in (0..a.len()).rev() {
        if a[i] != b[i] {
            return if a[i] < b[i] { -1 } else { 1 };
        }
    }
    0
}

fn model_shift_left(v: &[u32], n: u32) -> Vec<u32> {
    let words = (n / 32) as usize;
    let bits = n % 32;
    let mut out = vec![0u32; words];
    if bits == 0 {
        out.extend_from_slice(v);
    } else {
        let mut carry = 0u32;
        for &x in v {
            out.push((x << bits) | carry);
            carry = x >> (32 - bits);
        }
        if carry != 0 {
            out.push(carry);
        }
    }
    model_trim(out)
}

fn model_shift_right(v: &[u32], n: u32) -> Vec<u32> {
    let words = (n / 32) as usize;
    let bits = n % 32;
    if words >= v.len() {
        return vec![0];
    }
    let src = &v[words..];
    let mut out = Vec::with_capacity(src.len());
    if bits == 0 {
        out.extend_from_slice(src);
    } else {
        for i in 0..src.len() {
            let hi = if i + 1 < src.len() {
                src[i + 1] << (32 - bits)
            } else {
                0
            };
            out.push((src[i] >> bits) | hi);
        }
    }
    model_trim(out)
}

#[test]
fn bigint_ops_vs_model() {
    let mut state = 0x0bad_c0de_1234_5678u64;
    for case in 0..100_000u32 {
        let la = 1 + (splitmix64(&mut state) % 4) as usize;
        let lb = 1 + (splitmix64(&mut state) % 4) as usize;
        let mut a = Vec::with_capacity(la);
        let mut b = Vec::with_capacity(lb);
        for _ in 0..la {
            a.push(splitmix64(&mut state) as u32);
        }
        for _ in 0..lb {
            b.push(splitmix64(&mut state) as u32);
        }
        a = model_trim(a);
        b = model_trim(b);
        let m = (((splitmix64(&mut state) as u32) | 1) & 0x7fff_ffff) as i32;
        let add = ((splitmix64(&mut state) as u32) & 0x7fff_ffff) as i32;

        let mut ctx = DtoaCtx::new();
        let ar = model_from(&mut ctx, &a);
        let r = multadd(&mut ctx, ar, m, add).expect("multadd");
        assert_eq!(
            model_of(&ctx, r),
            model_mul_small(&a, m as u32, add as u32),
            "multadd case {case}"
        );

        let mut ctx = DtoaCtx::new();
        let ar = model_from(&mut ctx, &a);
        let br = model_from(&mut ctx, &b);
        let r = mult(&mut ctx, ar, br).expect("mult");
        assert_eq!(model_of(&ctx, r), model_mul(&a, &b), "mult case {case}");

        let mut ctx = DtoaCtx::new();
        let ar = model_from(&mut ctx, &a);
        let k = (splitmix64(&mut state) % 8) as i32;
        let r = pow5mult(&mut ctx, ar, k).expect("pow5mult");
        let got = model_of(&ctx, r);
        if k >= 0 {
            assert_eq!(got, model_pow5(&a, k as u32), "pow5mult case {case}");
        } else {
            assert_eq!(got, a, "pow5mult negative case {case}");
        }

        let mut ctx = DtoaCtx::new();
        let ar = model_from(&mut ctx, &a);
        let n = (splitmix64(&mut state) % 65) as u32;
        let r = lshift(&mut ctx, ar, n as i32).expect("lshift");
        assert_eq!(
            model_of(&ctx, r),
            model_shift_left(&a, n),
            "lshift case {case}"
        );

        let mut ctx = DtoaCtx::new();
        let ar = model_from(&mut ctx, &a);
        rshift(&mut ctx, ar, n as i32);
        assert_eq!(
            model_of(&ctx, ar),
            model_shift_right(&a, n),
            "rshift case {case}"
        );

        let mut ctx = DtoaCtx::new();
        let ar = model_from(&mut ctx, &a);
        let br = model_from(&mut ctx, &b);
        assert_eq!(
            cmp(&ctx, ar, br).signum(),
            model_cmp(&a, &b),
            "cmp case {case}"
        );
        let d = diff(&mut ctx, ar, br).expect("diff");
        let dsign = ctx.sign(d);
        let dmag = model_of(&ctx, d);
        let (want_mag, want_sign) = {
            let (big, small) = if model_cmp(&a, &b) >= 0 {
                (&a, &b)
            } else {
                (&b, &a)
            };
            let mut x = big.clone();
            let mut borrow = 0i64;
            for (i, slot) in x.iter_mut().enumerate() {
                let y = i64::from(small.get(i).copied().unwrap_or(0));
                let value = i64::from(*slot) - y - borrow;
                borrow = i64::from(value < 0);
                *slot = value as u32;
                if i >= small.len() && borrow == 0 {
                    break;
                }
            }
            (model_trim(x), i32::from(model_cmp(&a, &b) < 0))
        };
        assert_eq!(dmag, want_mag, "diff magnitude case {case}");
        assert_eq!(dsign, want_sign, "diff sign case {case}");
    }
}
