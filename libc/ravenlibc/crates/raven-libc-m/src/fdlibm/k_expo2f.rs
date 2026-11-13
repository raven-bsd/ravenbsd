// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/k_expo2f.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
use super::expf;

/* k is such that k*ln2 has minimal relative error and x - kln2 > log(FLT_MIN) */
const K: i32 = 235;

/* expf(x)/2 for x >= log(FLT_MAX), slightly better than 0.5f*expf(x/2)*expf(x/2) */
#[cfg_attr(assert_no_panic, no_panic::no_panic)]
pub(crate) fn k_expo2f(x: f32) -> f32 {
    let k_ln2 = f32::from_bits(0x4322e3bc);
    /* note that k is odd and scale*scale overflows */
    let scale = f32::from_bits(((0x7f + K / 2) as u32) << 23);
    /* exp(x - k ln2) * 2**(k-1) */
    expf(x - k_ln2) * scale * scale
}
