// SPDX-License-Identifier: MIT
// Copyright (c) 2018 Jorge Aparicio
// RavenBSD: transpiled from rust:library/compiler-builtins/libm/src/math/modff.rs @ 8bab26f4f68e0e26f0bb7960be334d5b520ea452
#[cfg_attr(assert_no_panic, no_panic::no_panic)]
pub fn modff(x: f32) -> (f32, f32) {
    let rv2: f32;
    let mut u: u32 = x.to_bits();
    let mask: u32;
    let e = (((u >> 23) & 0xff) as i32) - 0x7f;

    /* no fractional part */
    if e >= 23 {
        rv2 = x;
        if e == 0x80 && (u << 9) != 0 {
            /* nan */
            return (x, rv2);
        }
        u &= 0x80000000;
        return (f32::from_bits(u), rv2);
    }
    /* no integral part */
    if e < 0 {
        u &= 0x80000000;
        rv2 = f32::from_bits(u);
        return (x, rv2);
    }

    mask = 0x007fffff >> e;
    if (u & mask) == 0 {
        rv2 = x;
        u &= 0x80000000;
        return (f32::from_bits(u), rv2);
    }
    u &= !mask;
    rv2 = f32::from_bits(u);
    return (x - rv2, rv2);
}
